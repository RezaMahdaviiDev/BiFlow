//! Privileged supervision of the `OpenVPN` side tunnel.
//!
//! Two invariants drive every decision in this module.
//!
//! **`OpenVPN` never touches the routing table.** The helper always passes
//! `--route-noexec`, so the daemon brings its interface up and nothing else.
//! Every route that ends up in the kernel is installed here, from a list this
//! module validated, and a default route is rejected outright. A profile that
//! carries `redirect-gateway`, or a server that pushes one, therefore cannot
//! take the machine offline — which is the whole reason the side tunnel is
//! opt-in beside Hiddify instead of replacing it.
//!
//! **A `.ovpn` file is untrusted input to a root process.** `OpenVPN` can run
//! arbitrary commands through `up`, `down`, `plugin`, and friends. The helper
//! audits the profile for those directives and refuses to start when it finds
//! one, then pins `--script-security 0` on the command line as a second
//! barrier.
//!
//! Selected traffic reaches the tunnel through a marked policy-routing table
//! (Linux) or interface binding (Windows), never through the main table's
//! default route. See ADR 0067.

use super::{redact, HelperServiceError, Supervisor};
use ipnet::IpNet;
use iran_split_ipc::{OpenVpnRequest, OpenVpnStatus};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

/// Directives that let a profile run code, or steal the interface the helper
/// is about to own. Any of them aborts the start.
const FORBIDDEN_DIRECTIVES: [&str; 16] = [
    "up",
    "down",
    "up-restart",
    "route-up",
    "route-pre-down",
    "ipchange",
    "learn-address",
    "tls-verify",
    "auth-user-pass-verify",
    "client-connect",
    "client-disconnect",
    "plugin",
    "script-security",
    "daemon",
    "log",
    "log-append",
];

const DEVICE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const OPENVPN_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_OBSERVED_ROUTES: usize = 64;

/// Facts read out of the daemon's own output while it negotiates.
#[derive(Debug, Default)]
struct Observations {
    server_endpoint: Option<String>,
    pushed_routes: Vec<IpNet>,
    initialized: bool,
    fatal: Option<String>,
}

/// The running side tunnel and every piece of state the helper must undo.
#[derive(Debug)]
pub(crate) struct RunningOpenVpn {
    child: Child,
    device: String,
    routing_mark: u32,
    routing_table: u32,
    routes: Vec<IpNet>,
    policy_installed: bool,
    local_address: Option<String>,
    server_endpoint: Option<String>,
    started_at: String,
}

/// What [`Supervisor::settle_openvpn`] learned once the tunnel was usable.
#[derive(Debug)]
struct SettledOpenVpn {
    routes: Vec<IpNet>,
    policy_installed: bool,
    local_address: Option<String>,
    server_endpoint: Option<String>,
}

#[derive(Debug)]
struct ProfileFacts {
    remote_hosts: Vec<String>,
}

impl Supervisor {
    /// Starts the `OpenVPN` side tunnel and installs only its scoped routes.
    ///
    /// # Errors
    ///
    /// Returns [`HelperServiceError::OpenVpn`] when the request is malformed,
    /// the profile is missing or carries a script directive, the binary cannot
    /// be found or spawned, the tunnel does not come up before the timeout, or
    /// a scoped route cannot be installed.
    pub async fn start_openvpn(
        &self,
        request: &OpenVpnRequest,
    ) -> Result<OpenVpnStatus, HelperServiceError> {
        request
            .validate()
            .map_err(|error| HelperServiceError::OpenVpn(error.to_string()))?;
        // A second start would orphan the first tunnel's routes, so the old
        // one is torn down first rather than refused.
        self.stop_openvpn().await?;

        let facts = audit_profile(&request.profile)?;
        let binary = resolve_binary(request.executable.as_deref())?;
        if let Some(auth) = request.auth_file.as_deref() {
            ensure_regular_file(auth, "auth file")?;
        }

        let mut command = Command::new(&binary);
        command
            .args(build_arguments(request))
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_openvpn_spawn(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| HelperServiceError::OpenVpn(redact(&error.to_string())))?;

        let observations = Arc::new(Mutex::new(Observations::default()));
        if let Some(stdout) = child.stdout.take() {
            spawn_observer(stdout, Arc::clone(&observations), Arc::clone(&self.logs));
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_observer(stderr, Arc::clone(&observations), Arc::clone(&self.logs));
        }

        let outcome = self
            .settle_openvpn(request, &mut child, &observations, &facts)
            .await;
        match outcome {
            Ok(settled) => {
                let running = RunningOpenVpn {
                    child,
                    device: request.device.clone(),
                    routing_mark: request.routing_mark,
                    routing_table: request.routing_table,
                    routes: settled.routes,
                    policy_installed: settled.policy_installed,
                    local_address: settled.local_address,
                    server_endpoint: settled.server_endpoint,
                    started_at: super::now_string(),
                };
                let status = status_of(Some(&running));
                *self.openvpn.lock().await = Some(running);
                self.push_log(
                    "info",
                    "openvpn_started",
                    BTreeMap::from([
                        ("device".into(), request.device.clone()),
                        ("routes".into(), status.routes.len().to_string()),
                    ]),
                )
                .await;
                Ok(status)
            }
            Err(error) => {
                // A half-started tunnel is worse than none: the device may
                // exist with no routes while the desktop believes it failed.
                if let Err(cause) = terminate(&mut child).await {
                    self.push_log(
                        "warn",
                        "openvpn_rollback_kill_failed",
                        BTreeMap::from([("cause".into(), redact(&cause.to_string()))]),
                    )
                    .await;
                }
                revert_scoped_routes(&request.device, &[]).await;
                revert_policy_routing(request.routing_mark, request.routing_table).await;
                self.push_log(
                    "error",
                    "openvpn_start_failed",
                    BTreeMap::from([("cause".into(), redact(&error.to_string()))]),
                )
                .await;
                Err(error)
            }
        }
    }

    /// Waits for the tunnel to be usable, then installs its scoped routes.
    async fn settle_openvpn(
        &self,
        request: &OpenVpnRequest,
        child: &mut Child,
        observations: &Arc<Mutex<Observations>>,
        facts: &ProfileFacts,
    ) -> Result<SettledOpenVpn, HelperServiceError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(request.start_timeout_seconds);
        let local_address = loop {
            if let Some(exit) = child
                .try_wait()
                .map_err(|error| HelperServiceError::OpenVpn(error.to_string()))?
            {
                let reported = observations.lock().await.fatal.clone();
                return Err(HelperServiceError::OpenVpn(reported.unwrap_or_else(|| {
                    format!("openvpn exited early with status {exit}")
                })));
            }
            if let Some(fatal) = observations.lock().await.fatal.clone() {
                return Err(HelperServiceError::OpenVpn(fatal));
            }
            // Both signals matter: the interface can carry an address a moment
            // before the daemon finishes negotiating, and the PUSH_REPLY that
            // names the scoped routes only lands at the end of that handshake.
            if observations.lock().await.initialized {
                if let Some(address) = device_address(&request.device).await {
                    break address;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(HelperServiceError::OpenVpn(format!(
                    "openvpn did not bring {} up within {} seconds",
                    request.device, request.start_timeout_seconds
                )));
            }
            tokio::time::sleep(DEVICE_POLL_INTERVAL).await;
        };

        let observed = observations.lock().await;
        let mut routes = Vec::new();
        // "Only its own IP": the tunnel's own network is what the side tunnel
        // carries by default. Everything else stays on the existing split.
        if let Some(network) = local_address.trunc_network() {
            routes.push(network);
        }
        if request.pull_routes {
            routes.extend(observed.pushed_routes.iter().copied());
        }
        let server_endpoint = observed
            .server_endpoint
            .clone()
            .or_else(|| facts.remote_hosts.first().cloned());
        drop(observed);

        for extra in &request.tunnel_routes {
            if let Ok(network) = extra.trim().parse::<IpNet>() {
                routes.push(network);
            }
        }
        routes.retain(|network| network.prefix_len() > 0);
        routes.sort_unstable_by_key(ToString::to_string);
        routes.dedup();
        routes.truncate(MAX_OBSERVED_ROUTES);

        install_scoped_routes(&request.device, &routes).await?;
        // A leftover rule from a crashed session would silently shadow the new
        // one, so the mark and table are cleared before they are claimed.
        revert_policy_routing(request.routing_mark, request.routing_table).await;
        let policy_installed =
            install_policy_routing(&request.device, request.routing_mark, request.routing_table)
                .await?;

        Ok(SettledOpenVpn {
            routes,
            policy_installed,
            local_address: Some(local_address.address.to_string()),
            server_endpoint,
        })
    }

    /// Stops the side tunnel and removes every route the helper installed.
    ///
    /// # Errors
    ///
    /// Returns [`HelperServiceError::OpenVpn`] when the process cannot be
    /// inspected or terminated. Route removal never fails the call: a route
    /// that is already gone is the desired end state.
    pub async fn stop_openvpn(&self) -> Result<OpenVpnStatus, HelperServiceError> {
        let running = self.openvpn.lock().await.take();
        let Some(mut running) = running else {
            return Ok(OpenVpnStatus::default());
        };
        revert_scoped_routes(&running.device, &running.routes).await;
        if running.policy_installed {
            revert_policy_routing(running.routing_mark, running.routing_table).await;
        }
        terminate(&mut running.child).await?;
        self.push_log(
            "info",
            "openvpn_stopped",
            BTreeMap::from([("device".into(), running.device.clone())]),
        )
        .await;
        Ok(OpenVpnStatus::default())
    }

    /// Reports the side tunnel's live state, forgetting a daemon that died.
    pub async fn openvpn_status(&self) -> OpenVpnStatus {
        let mut current = self.openvpn.lock().await;
        if let Some(running) = current.as_mut() {
            if matches!(running.child.try_wait(), Ok(Some(_))) {
                let device = running.device.clone();
                let routes = running.routes.clone();
                let mark = running.routing_mark;
                let table = running.routing_table;
                let policy = running.policy_installed;
                *current = None;
                drop(current);
                // The daemon is gone but its interface routes are not, and a
                // stale route to a dead device blackholes traffic.
                revert_scoped_routes(&device, &routes).await;
                if policy {
                    revert_policy_routing(mark, table).await;
                }
                self.push_log(
                    "warn",
                    "openvpn_exited",
                    BTreeMap::from([("device".into(), device)]),
                )
                .await;
                return OpenVpnStatus {
                    last_error: Some("openvpn exited unexpectedly".into()),
                    ..OpenVpnStatus::default()
                };
            }
        }
        status_of(current.as_ref())
    }
}

fn status_of(running: Option<&RunningOpenVpn>) -> OpenVpnStatus {
    running.map_or_else(OpenVpnStatus::default, |running| OpenVpnStatus {
        running: true,
        pid: running.child.id(),
        device: Some(running.device.clone()),
        local_address: running.local_address.clone(),
        server_endpoint: running.server_endpoint.clone(),
        routes: running.routes.iter().map(ToString::to_string).collect(),
        started_at: Some(running.started_at.clone()),
        last_error: None,
    })
}

/// Builds the command line. The overriding flags follow `--config` on purpose:
/// `OpenVPN` lets a later occurrence win, so a profile cannot re-enable what
/// the helper switched off.
fn build_arguments(request: &OpenVpnRequest) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--config".into(), request.profile.clone().into()];
    args.extend([
        // Nothing this daemon says may reach the routing table.
        "--route-noexec".into(),
        "--script-security".into(),
        "0".into(),
        "--dev-type".into(),
        "tun".into(),
        "--dev".into(),
        request.device.clone().into(),
        "--verb".into(),
        "3".into(),
        "--connect-retry-max".into(),
        "3".into(),
        "--auth-nocache".into(),
        "--remap-usr1".into(),
        "SIGTERM".into(),
    ]);
    // Belt and braces: even with --route-noexec these keep the daemon from
    // rewriting DNS or announcing itself as the gateway.
    for filter in [
        "redirect-gateway",
        "redirect-gateway-ipv6",
        "route 0.0.0.0",
        "route 128.0.0.0 128.0.0.0",
        "route-ipv6 ::/0",
        "dhcp-option DNS",
        "dhcp-option DNS6",
        "block-outside-dns",
    ] {
        args.push("--pull-filter".into());
        args.push("ignore".into());
        args.push(filter.into());
    }
    if !request.pull_routes {
        args.push("--route-nopull".into());
    }
    if let Some(auth) = request.auth_file.as_ref() {
        args.push("--auth-user-pass".into());
        args.push(auth.clone().into());
    }
    args
}

/// Reads the profile, rejecting anything that would run code as root.
fn audit_profile(path: &Path) -> Result<ProfileFacts, HelperServiceError> {
    ensure_regular_file(path, "profile")?;
    let text = fs::read_to_string(path).map_err(|error| {
        HelperServiceError::OpenVpn(format!("profile could not be read: {}", error.kind()))
    })?;
    let mut remote_hosts = Vec::new();
    let mut inline_block: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(open) = inline_block.as_deref() {
            if line.eq_ignore_ascii_case(&format!("</{open}>")) {
                inline_block = None;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(tag) = line
            .strip_prefix('<')
            .and_then(|rest| rest.strip_suffix('>'))
        {
            if !tag.starts_with('/') {
                inline_block = Some(tag.to_ascii_lowercase());
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(directive) = parts.next() else {
            continue;
        };
        let directive = directive.trim_start_matches("--").to_ascii_lowercase();
        if FORBIDDEN_DIRECTIVES.contains(&directive.as_str()) {
            return Err(HelperServiceError::OpenVpn(format!(
                "profile uses the '{directive}' directive, which would run commands as root"
            )));
        }
        if directive == "remote" {
            if let Some(host) = parts.next() {
                remote_hosts.push(host.to_owned());
            }
        }
    }
    if inline_block.is_some() {
        return Err(HelperServiceError::OpenVpn(
            "profile has an unterminated inline block".into(),
        ));
    }
    if remote_hosts.is_empty() {
        return Err(HelperServiceError::OpenVpn(
            "profile declares no remote server".into(),
        ));
    }
    Ok(ProfileFacts { remote_hosts })
}

fn ensure_regular_file(path: &Path, label: &str) -> Result<(), HelperServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        HelperServiceError::OpenVpn(format!("{label} is unreadable: {}", error.kind()))
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(HelperServiceError::OpenVpn(format!(
            "{label} must be a regular, non-symlink file"
        )));
    }
    Ok(())
}

fn resolve_binary(configured: Option<&Path>) -> Result<PathBuf, HelperServiceError> {
    if let Some(path) = configured {
        ensure_regular_file(path, "openvpn binary")?;
        return Ok(path.to_path_buf());
    }
    candidate_binaries()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            HelperServiceError::OpenVpn(
                "the openvpn binary was not found; install OpenVPN or set its path".into(),
            )
        })
}

#[cfg(unix)]
fn candidate_binaries() -> Vec<PathBuf> {
    [
        "/usr/sbin/openvpn",
        "/usr/bin/openvpn",
        "/sbin/openvpn",
        "/bin/openvpn",
        "/usr/local/sbin/openvpn",
        "/usr/local/bin/openvpn",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(windows)]
fn candidate_binaries() -> Vec<PathBuf> {
    [
        r"C:\Program Files\OpenVPN\bin\openvpn.exe",
        r"C:\Program Files (x86)\OpenVPN\bin\openvpn.exe",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

/// The address the tunnel assigned locally, with its prefix.
#[derive(Debug, Clone, Copy)]
struct DeviceAddress {
    address: IpAddr,
    prefix_len: u8,
}

impl DeviceAddress {
    /// The network the tunnel itself lives on — the one route the side tunnel
    /// always carries.
    fn trunc_network(self) -> Option<IpNet> {
        if self.prefix_len == 0 {
            return None;
        }
        IpNet::new(self.address, self.prefix_len)
            .ok()
            .map(|network| network.trunc())
    }
}

#[cfg(unix)]
async fn device_address(device: &str) -> Option<DeviceAddress> {
    if !Path::new("/sys/class/net").join(device).exists() {
        return None;
    }
    let output = ip_command(&["-o", "-4", "addr", "show", "dev", device]).await?;
    parse_ip_addr_output(&output)
}

#[cfg(windows)]
async fn device_address(device: &str) -> Option<DeviceAddress> {
    let output = run_capture(
        "netsh",
        &[
            "interface",
            "ipv4",
            "show",
            "addresses",
            &format!("name={device}"),
        ],
    )
    .await?;
    parse_netsh_addresses(&output)
}

/// Reads `inet 10.8.0.6/24` out of one `ip -o -4 addr` line.
fn parse_ip_addr_output(output: &str) -> Option<DeviceAddress> {
    output
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|pair| {
            if pair[0] != "inet" {
                return None;
            }
            let (address, prefix) = pair[1].split_once('/')?;
            Some(DeviceAddress {
                address: address.parse().ok()?,
                prefix_len: prefix.parse().ok()?,
            })
        })
}

/// Reads the address and mask out of `netsh interface ipv4 show addresses`.
#[cfg(windows)]
fn parse_netsh_addresses(output: &str) -> Option<DeviceAddress> {
    let mut address = None;
    let mut prefix_len = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("IP Address:") {
            address = value.trim().parse::<IpAddr>().ok();
        } else if let Some(value) = line.strip_prefix("Subnet Prefix:") {
            prefix_len = value
                .split('/')
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|bits| bits.parse::<u8>().ok());
        }
    }
    Some(DeviceAddress {
        address: address?,
        prefix_len: prefix_len?,
    })
}

#[cfg(unix)]
async fn install_scoped_routes(device: &str, routes: &[IpNet]) -> Result<(), HelperServiceError> {
    for network in routes {
        let target = network.to_string();
        if ip_command(&["route", "replace", &target, "dev", device])
            .await
            .is_none()
        {
            return Err(HelperServiceError::OpenVpn(format!(
                "route {target} could not be added to {device}"
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn install_scoped_routes(device: &str, routes: &[IpNet]) -> Result<(), HelperServiceError> {
    for network in routes {
        let family = if network.addr().is_ipv4() {
            "ipv4"
        } else {
            "ipv6"
        };
        let added = run_capture(
            "netsh",
            &[
                "interface",
                family,
                "add",
                "route",
                &network.to_string(),
                &format!("interface={device}"),
                "store=active",
            ],
        )
        .await;
        if added.is_none() {
            return Err(HelperServiceError::OpenVpn(format!(
                "route {network} could not be added to {device}"
            )));
        }
    }
    Ok(())
}

/// Adds the marked table that carries traffic Mihomo pins to `OpenVPN`.
///
/// The default route lives in a *separate* table that only marked packets can
/// reach, so nothing on the system follows it by accident.
#[cfg(unix)]
async fn install_policy_routing(
    device: &str,
    mark: u32,
    table: u32,
) -> Result<bool, HelperServiceError> {
    let table = table.to_string();
    let mark = format!("{mark:#x}");
    let installed = ip_command(&[
        "route", "replace", "default", "dev", device, "table", &table,
    ])
    .await
    .is_some()
        && ip_command(&[
            "rule", "add", "fwmark", &mark, "lookup", &table, "priority", "17800",
        ])
        .await
        .is_some();
    if !installed {
        return Err(HelperServiceError::OpenVpn(
            "the OpenVPN policy-routing table could not be installed".into(),
        ));
    }
    Ok(true)
}

#[cfg(windows)]
async fn install_policy_routing(
    _device: &str,
    _mark: u32,
    _table: u32,
) -> Result<bool, HelperServiceError> {
    // Windows has no fwmark. Mihomo binds its OpenVPN outbound to the
    // interface instead, so there is no policy table to install.
    Ok(false)
}

#[cfg(unix)]
async fn revert_scoped_routes(device: &str, routes: &[IpNet]) {
    // A route or device that is already gone is the desired end state, so
    // these deliberately ignore their result instead of failing the teardown.
    for network in routes {
        drop(ip_command(&["route", "del", &network.to_string(), "dev", device]).await);
    }
    if Path::new("/sys/class/net").join(device).exists() {
        drop(ip_command(&["link", "delete", "dev", device]).await);
    }
}

#[cfg(windows)]
async fn revert_scoped_routes(device: &str, routes: &[IpNet]) {
    for network in routes {
        let family = if network.addr().is_ipv4() {
            "ipv4"
        } else {
            "ipv6"
        };
        // A route that is already gone is the desired end state, so a failure
        // here is not worth failing the teardown over.
        drop(
            run_capture(
                "netsh",
                &[
                    "interface",
                    family,
                    "delete",
                    "route",
                    &network.to_string(),
                    &format!("interface={device}"),
                ],
            )
            .await,
        );
    }
}

#[cfg(unix)]
async fn revert_policy_routing(mark: u32, table: u32) {
    let table = table.to_string();
    let mark = format!("{mark:#x}");
    // Both are idempotent teardowns: "rule does not exist" is success here.
    drop(ip_command(&["rule", "del", "fwmark", &mark, "lookup", &table]).await);
    drop(ip_command(&["route", "flush", "table", &table]).await);
}

#[cfg(windows)]
async fn revert_policy_routing(_mark: u32, _table: u32) {}

#[cfg(unix)]
async fn ip_command(args: &[&str]) -> Option<String> {
    let binary = [Path::new("/usr/sbin/ip"), Path::new("/usr/bin/ip")]
        .into_iter()
        .find(|path| path.is_file())?;
    let output = Command::new(binary)
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(windows)]
async fn run_capture(binary: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Rebuilds the minimum environment `OpenVPN` needs after `env_clear`.
#[cfg(unix)]
fn apply_openvpn_spawn(command: &mut Command) {
    command.env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin");
}

/// Rebuilds the minimum environment `OpenVPN` needs after `env_clear`.
///
/// A cleared environment on Windows costs the process `SYSTEMROOT`, without
/// which it cannot start at all, so the same variables the Mihomo spawn
/// restores are restored here — and the console window is hidden, since the
/// helper runs as a service.
#[cfg(windows)]
fn apply_openvpn_spawn(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let system_root = std::env::var("SYSTEMROOT").unwrap_or_else(|_| r"C:\Windows".into());
    let system_drive = std::env::var("SYSTEMDRIVE").unwrap_or_else(|_| r"C:".into());
    command
        .env("SYSTEMROOT", &system_root)
        .env("SystemRoot", &system_root)
        .env("WINDIR", &system_root)
        .env("SYSTEMDRIVE", system_drive)
        .env("PATHEXT", ".COM;.EXE;.BAT;.CMD")
        .creation_flags(CREATE_NO_WINDOW);
}

async fn terminate(child: &mut Child) -> Result<(), HelperServiceError> {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return Ok(());
    }
    child
        .start_kill()
        .map_err(|error| HelperServiceError::OpenVpn(error.to_string()))?;
    tokio::time::timeout(OPENVPN_STOP_TIMEOUT, child.wait())
        .await
        .map_err(|_| HelperServiceError::OpenVpn("openvpn did not stop in time".into()))?
        .map_err(|error| HelperServiceError::OpenVpn(error.to_string()))?;
    Ok(())
}

/// Mirrors the daemon's output into the service log and harvests the two facts
/// the helper needs: the real peer address and the scoped routes it pushed.
///
/// `OpenVPN` prints credentials-adjacent detail at higher verbosities, so every
/// line goes through [`redact`] before it is stored.
fn spawn_observer<R>(
    stream: R,
    observations: Arc<Mutex<Observations>>,
    logs: Arc<Mutex<VecDeque<iran_split_ipc::ServiceLogEntry>>>,
) where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let safe = redact(&line);
            observe(&observations, &line).await;
            let mut logs = logs.lock().await;
            if logs.len() == super::MAX_LOG_ENTRIES {
                logs.pop_front();
            }
            logs.push_back(iran_split_ipc::ServiceLogEntry {
                timestamp: super::now_string(),
                level: "info".into(),
                event: "openvpn_output".into(),
                fields: BTreeMap::from([("message".into(), safe)]),
            });
        }
    });
}

async fn observe(observations: &Arc<Mutex<Observations>>, line: &str) {
    if let Some(endpoint) = peer_address(line) {
        observations.lock().await.server_endpoint = Some(endpoint);
    }
    if let Some(routes) = pushed_routes(line) {
        let mut guard = observations.lock().await;
        for route in routes {
            if guard.pushed_routes.len() < MAX_OBSERVED_ROUTES
                && !guard.pushed_routes.contains(&route)
            {
                guard.pushed_routes.push(route);
            }
        }
    }
    if line.contains("Initialization Sequence Completed") {
        observations.lock().await.initialized = true;
    }
    if let Some(reason) = fatal_reason(line) {
        observations.lock().await.fatal = Some(reason);
    }
}

/// Extracts the peer from `Peer Connection Initiated with [AF_INET]1.2.3.4:1194`.
fn peer_address(line: &str) -> Option<String> {
    let rest = line.split("Peer Connection Initiated with ").nth(1)?;
    let rest = rest.trim_start_matches('[');
    let rest = rest.split_once(']').map_or(rest, |(_, tail)| tail);
    let host = rest.rsplit_once(':').map_or(rest, |(head, _)| head);
    host.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

/// Extracts scoped `route` directives from a `PUSH_REPLY` line, dropping any
/// that would amount to a default route.
fn pushed_routes(line: &str) -> Option<Vec<IpNet>> {
    let reply = line.split("PUSH_REPLY,").nth(1)?;
    let reply = reply.trim_end_matches('\'');
    let mut routes = Vec::new();
    for entry in reply.split(',') {
        let mut parts = entry.split_whitespace();
        if parts.next() != Some("route") {
            continue;
        }
        let Some(network) = parts
            .next()
            .and_then(|value| value.parse::<Ipv4Addr>().ok())
        else {
            continue;
        };
        let prefix_len = parts
            .next()
            .and_then(|value| value.parse::<Ipv4Addr>().ok())
            .map_or(32, |mask| u32::from(mask).count_ones());
        let Ok(prefix_len) = u8::try_from(prefix_len) else {
            continue;
        };
        if prefix_len == 0 {
            continue;
        }
        if let Ok(parsed) = IpNet::new(IpAddr::V4(network), prefix_len) {
            routes.push(parsed.trunc());
        }
    }
    (!routes.is_empty()).then_some(routes)
}

fn fatal_reason(line: &str) -> Option<String> {
    const FATAL_MARKERS: [&str; 4] = [
        "AUTH_FAILED",
        "Cannot open TUN/TAP dev",
        "Options error",
        "Exiting due to fatal error",
    ];
    FATAL_MARKERS
        .into_iter()
        .find(|marker| line.contains(marker))
        .map(|marker| format!("openvpn reported {marker}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn request(profile: &Path) -> OpenVpnRequest {
        OpenVpnRequest {
            profile: profile.to_path_buf(),
            auth_file: None,
            executable: None,
            device: "biflow-ovpn".into(),
            pull_routes: true,
            tunnel_routes: vec!["192.168.44.0/24".into()],
            routing_mark: 0x0000_b1f0,
            routing_table: 178,
            start_timeout_seconds: 30,
        }
    }

    fn write_profile(directory: &Path, body: &str) -> PathBuf {
        let path = directory.join("profile.ovpn");
        let mut file = fs::File::create(&path).expect("create profile");
        file.write_all(body.as_bytes()).expect("write profile");
        path
    }

    #[test]
    fn arguments_never_let_the_profile_own_routes_or_scripts() {
        let directory = tempfile::tempdir().expect("tempdir");
        let profile = write_profile(directory.path(), "client\nremote vpn.example.com 1194\n");
        let arguments = build_arguments(&request(&profile))
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.contains(&"--route-noexec".to_owned()));
        assert!(arguments.contains(&"--script-security".to_owned()));
        assert!(arguments.contains(&"redirect-gateway".to_owned()));
        assert!(arguments.contains(&"route 0.0.0.0".to_owned()));
        assert!(arguments.contains(&"dhcp-option DNS".to_owned()));
        // --config must come first so every later flag overrides the profile.
        assert_eq!(arguments.first().map(String::as_str), Some("--config"));
        let device = arguments
            .iter()
            .position(|value| value == "--dev")
            .expect("device flag");
        assert_eq!(arguments[device + 1], "biflow-ovpn");
    }

    #[test]
    fn route_nopull_is_added_only_when_pulling_is_off() {
        let directory = tempfile::tempdir().expect("tempdir");
        let profile = write_profile(directory.path(), "client\nremote vpn.example.com 1194\n");
        let mut value = request(&profile);
        value.pull_routes = false;
        let arguments = build_arguments(&value)
            .into_iter()
            .map(|item| item.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.contains(&"--route-nopull".to_owned()));
    }

    #[test]
    fn profile_audit_rejects_script_directives() {
        let directory = tempfile::tempdir().expect("tempdir");
        let hostile = write_profile(
            directory.path(),
            "client\nremote vpn.example.com 1194\nup /tmp/pwn.sh\n",
        );
        let error = audit_profile(&hostile).expect_err("script directive");
        assert!(error.to_string().contains("run commands as root"));

        let plugin = write_profile(
            directory.path(),
            "client\nremote vpn.example.com 1194\nplugin /tmp/evil.so\n",
        );
        assert!(audit_profile(&plugin).is_err());
    }

    #[test]
    fn profile_audit_accepts_a_normal_profile_and_reads_its_remote() {
        let directory = tempfile::tempdir().expect("tempdir");
        let profile = write_profile(
            directory.path(),
            concat!(
                "client\n",
                "dev tun\n",
                "proto udp\n",
                "remote vpn.example.com 1194\n",
                "redirect-gateway def1\n",
                "<ca>\n",
                "up should-not-be-read\n",
                "</ca>\n",
            ),
        );
        let facts = audit_profile(&profile).expect("valid profile");
        assert_eq!(facts.remote_hosts, ["vpn.example.com"]);
    }

    #[test]
    fn profile_audit_requires_a_remote() {
        let directory = tempfile::tempdir().expect("tempdir");
        let profile = write_profile(directory.path(), "client\ndev tun\n");
        assert!(audit_profile(&profile).is_err());
    }

    #[test]
    fn pushed_default_routes_are_dropped_and_scoped_ones_kept() {
        let line = "PUSH: Received control message: 'PUSH_REPLY,route 10.8.0.0 255.255.255.0,route 0.0.0.0 0.0.0.0,redirect-gateway def1,ping 10'";
        let routes = pushed_routes(line).expect("routes");
        assert_eq!(
            routes.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["10.8.0.0/24"]
        );
    }

    #[test]
    fn peer_address_is_read_from_the_connection_line() {
        let line =
            "Mon Aug 10 12:00:00 2026 Peer Connection Initiated with [AF_INET]203.0.113.9:1194";
        assert_eq!(peer_address(line).as_deref(), Some("203.0.113.9"));
        assert!(peer_address("nothing here").is_none());
    }

    #[test]
    fn device_address_network_is_truncated_to_its_prefix() {
        let parsed =
            parse_ip_addr_output("7: biflow-ovpn    inet 10.8.0.6/24 scope global biflow-ovpn")
                .expect("address");
        assert_eq!(parsed.address.to_string(), "10.8.0.6");
        assert_eq!(
            parsed.trunc_network().map(|net| net.to_string()),
            Some("10.8.0.0/24".to_owned())
        );
        let host_only =
            parse_ip_addr_output("7: biflow-ovpn    inet 10.8.0.6/32 scope global biflow-ovpn")
                .expect("address");
        assert_eq!(
            host_only.trunc_network().map(|net| net.to_string()),
            Some("10.8.0.6/32".to_owned())
        );
    }

    #[test]
    fn fatal_markers_are_reported_verbatim_enough_to_diagnose() {
        assert!(fatal_reason("AUTH_FAILED").is_some());
        assert!(fatal_reason("Initialization Sequence Completed").is_none());
    }
}
