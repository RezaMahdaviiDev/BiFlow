# Offline rule snapshot

These immutable installer inputs were imported on 2026-08-12 from the working
Linux stack at `/home/devlife/dev/dariush/clash/rules/`:

| File | Lines | SHA-256 |
|---|---:|---|
| `private.txt` | 19 | `aed134cc43c2414cb3df5a10fcb3e215e64fac0249579a112c163674df4ddd36` |
| `iran-domains.txt` | 62,829 | `ae533f8bf147877bb97efd24a3dd708695f10289462d0746c30af0d9442f2581` |
| `iran-networks.txt` | 2,880 | `70b39249a8ae55e03896a47814f1b4373aacccf910c99cfa1dac0ee472a0959c` |

The installed copy is never modified. Live refreshes are validated and written
to the application data directory, and a failed refresh keeps the last known
good cache. Lines beginning with `#` are metadata. Domain entries may use the
Mihomo text-provider `+.` suffix form.
