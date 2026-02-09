# Coordinator Configuration

The coordinator can be configured using a TOML config file.

## Config File Locations

The config is loaded from the first existing file in this order:
1. `./ruleco.toml` (in the current working directory)
2. `~/.config/ruleco/config.toml` (XDG config home)

If no config file is found, defaults are used.

## Config Format

```toml
[coordinator]
namespace = "My_Namespace"
timeout_interval = 10
```

### Settings

| Setting            | Type   | Default             | Description                                                 |
|--------------------|--------|---------------------|-------------------------------------------------------------|
| `namespace`        | String | `Default_Namespace` | The coordinator's namespace. Set to `""` to auto-detect from hostname |
| `timeout_interval` | u32    | `10`                | Timeout interval in seconds for device communication checks |

### Namespace Resolution

The namespace is resolved in this order:

1. **If config has `namespace = ""` (empty string)** → Uses the machine's hostname automatically
2. **If config has `namespace = "Custom"` → Uses "Custom"**
3. **If config is missing or no config file exists** → Uses `"Default_Namespace"`

This is useful for lab environments where each coordinator runs on a separate machine. By setting `namespace = ""`, each coordinator automatically gets a unique namespace based on its hostname.

## Examples

### Using hostname auto-detection (recommended for lab setups)

Create a `ruleco.toml` file:

```toml
[coordinator]
namespace = ""
timeout_interval = 15
```

The coordinator will automatically use the machine's hostname as the namespace. This is ideal for one-coordinator-per-machine setups.

### Using a custom namespace

```toml
[coordinator]
namespace = "Lab_Machine_1"
timeout_interval = 15
```

### No config file

If no config file is found, uses default settings:

- namespace: `Default_Namespace`
- timeout_interval: `10` seconds

## Running the Coordinator

```bash
cargo run -p ruleco-coordinator
```

The coordinator will output the loaded configuration at startup:

```
Using namespace: hostname.example.com
Using timeout interval: 15 seconds
Coordinator started
```

Or with default settings (no config):

```
Using namespace: Default_Namespace
Using timeout interval: 10 seconds
Coordinator started
```
