import { zodResolver } from "@hookform/resolvers/zod";
import { Save } from "lucide-react";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import type { UseFormRegisterReturn } from "react-hook-form";
import { z } from "zod";
import { desktop } from "../api/desktop";
import type { AppConfig, ValidationIssue } from "../api/models";
import { useAppStore } from "../store/app";

const formSchema = z.object({
  hiddifyHost: z.literal("127.0.0.1"),
  hiddifyPort: z.coerce.number().int().min(1).max(65535),
  startTimeout: z.coerce.number().int().min(1).max(300),
  stopWithStack: z.boolean(),
  controllerPort: z.coerce.number().int().min(1).max(65535),
  mixedPort: z.coerce.number().int().min(1).max(65535),
  dnsPort: z.coerce.number().int().min(1).max(65535),
  tunName: z.string().min(1).max(64).regex(/^[a-zA-Z0-9_-]+$/),
  logLevel: z.enum(["error", "warn", "info", "debug"]),
  refreshMinutes: z.coerce.number().int().min(1),
  upstreamHours: z.coerce.number().int().min(1),
  launchAtLogin: z.boolean(),
  connectAtLaunch: z.boolean(),
  closeToTray: z.boolean(),
});

type FormValues = z.infer<typeof formSchema>;

function toValues(config: AppConfig): FormValues {
  return {
    hiddifyHost: "127.0.0.1",
    hiddifyPort: config.hiddify.port,
    startTimeout: config.hiddify.start_timeout_seconds,
    stopWithStack: config.hiddify.stop_with_stack,
    controllerPort: config.mihomo.controller_port,
    mixedPort: config.mihomo.mixed_port,
    dnsPort: config.mihomo.dns_port,
    tunName: config.mihomo.tun_name,
    logLevel: config.mihomo.log_level,
    refreshMinutes: config.rules.refresh_interval_minutes,
    upstreamHours: config.rules.upstream_refresh_hours,
    launchAtLogin: config.behavior.launch_at_login,
    connectAtLaunch: config.behavior.connect_at_launch,
    closeToTray: config.behavior.close_to_tray,
  };
}

function merge(config: AppConfig, values: FormValues): AppConfig {
  return {
    ...config,
    hiddify: {
      ...config.hiddify,
      host: values.hiddifyHost,
      port: values.hiddifyPort,
      start_timeout_seconds: values.startTimeout,
      stop_with_stack: values.stopWithStack,
    },
    mihomo: {
      ...config.mihomo,
      controller_port: values.controllerPort,
      mixed_port: values.mixedPort,
      dns_port: values.dnsPort,
      tun_name: values.tunName,
      log_level: values.logLevel,
    },
    rules: {
      refresh_interval_minutes: values.refreshMinutes,
      upstream_refresh_hours: values.upstreamHours,
    },
    behavior: {
      launch_at_login: values.launchAtLogin,
      connect_at_launch: values.connectAtLaunch,
      close_to_tray: values.closeToTray,
    },
  };
}

export function Settings({ settings }: { settings: AppConfig }) {
  const { saveSettings, actionPending } = useAppStore();
  const [issues, setIssues] = useState<ValidationIssue[]>([]);
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors, isDirty },
  } = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: toValues(settings),
  });

  useEffect(() => reset(toValues(settings)), [reset, settings]);

  const submit = handleSubmit(async (values) => {
    const draft = merge(settings, values);
    const validation = await desktop.validateSettings(draft);
    setIssues(validation);
    if (validation.length === 0) await saveSettings(draft);
  });

  return (
    <section aria-labelledby="settings-title" className="space-y-5">
      <header>
        <h1 id="settings-title" className="text-3xl font-semibold tracking-tight">
          Settings
        </h1>
        <p className="mt-2 text-muted">
          Advanced ports stay on loopback and are checked for conflicts before publication.
        </p>
      </header>

      <form onSubmit={(event) => void submit(event)} className="space-y-4">
        <Fieldset legend="Hiddify upstream">
          <Field label="Host" error={errors.hiddifyHost?.message}>
            <input {...register("hiddifyHost")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="SOCKS / mixed port" error={errors.hiddifyPort?.message}>
            <input type="number" {...register("hiddifyPort")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="Start timeout (seconds)" error={errors.startTimeout?.message}>
            <input type="number" {...register("startTimeout")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Check label="Stop Hiddify with stack" registration={register("stopWithStack")} />
        </Fieldset>

        <Fieldset legend="Mihomo and network">
          <Field label="Controller port" error={errors.controllerPort?.message}>
            <input type="number" {...register("controllerPort")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="Mixed port" error={errors.mixedPort?.message}>
            <input type="number" {...register("mixedPort")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="DNS port" error={errors.dnsPort?.message}>
            <input type="number" {...register("dnsPort")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="TUN name" error={errors.tunName?.message}>
            <input {...register("tunName")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="Log level" error={errors.logLevel?.message}>
            <select {...register("logLevel")} className="w-full rounded-xl border-ink/15 bg-canvas">
              <option value="error">Error</option>
              <option value="warn">Warning</option>
              <option value="info">Info</option>
              <option value="debug">Debug</option>
            </select>
          </Field>
        </Fieldset>

        <Fieldset legend="Behavior and refresh">
          <Field label="Custom rule refresh (minutes)" error={errors.refreshMinutes?.message}>
            <input type="number" {...register("refreshMinutes")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <Field label="Upstream refresh (hours)" error={errors.upstreamHours?.message}>
            <input type="number" {...register("upstreamHours")} className="w-full rounded-xl border-ink/15 bg-canvas" />
          </Field>
          <div className="space-y-3">
            <Check label="Launch at login" registration={register("launchAtLogin")} />
            <Check label="Connect at launch" registration={register("connectAtLaunch")} />
            <Check label="Close window to tray" registration={register("closeToTray")} />
          </div>
        </Fieldset>

        {issues.length > 0 ? (
          <ul className="rounded-xl border border-danger/20 bg-danger/5 p-4 text-sm text-danger" role="alert">
            {issues.map((issue) => (
              <li key={`${issue.field}-${issue.code}`}>{issue.field}: {issue.message}</li>
            ))}
          </ul>
        ) : null}

        <button
          disabled={actionPending || !isDirty}
          className="inline-flex items-center gap-2 rounded-xl bg-brand px-5 py-3 font-semibold text-white disabled:opacity-50"
        >
          <Save size={18} aria-hidden /> Save settings
        </button>
      </form>
    </section>
  );
}

function Fieldset({ legend, children }: { legend: string; children: React.ReactNode }) {
  return (
    <fieldset className="grid gap-4 rounded-2xl border border-ink/10 bg-surface p-5 sm:grid-cols-2">
      <legend className="px-2 font-semibold">{legend}</legend>
      {children}
    </fieldset>
  );
}

function Field({ label, error, children }: { label: string; error?: string; children: React.ReactNode }) {
  return (
    <label className="block text-sm font-medium">
      <span className="mb-1.5 block">{label}</span>
      {children}
      {error ? <span className="mt-1 block text-xs text-danger">{error}</span> : null}
    </label>
  );
}

function Check({ label, registration }: { label: string; registration: UseFormRegisterReturn }) {
  return (
    <label className="flex items-center gap-3 text-sm font-medium">
      <input type="checkbox" {...registration} className="rounded border-ink/20 text-brand focus:ring-brand" />
      {label}
    </label>
  );
}
