# color-quantize ( AOD_ColorQuantize )

Reduces image colors with k-means clustering.

This is the After Effects plugin **AOD_ColorQuantize**, which provides the **ColorQuantize.aex** plugin file for Adobe After Effects.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.

## Performance Log (Debug)

Performance logging is enabled automatically when this plugin is built in debug mode.
When built in release mode, performance logging is disabled.

- Windows: logs are emitted via `OutputDebugStringW`, so you can inspect them in DebugView.
- Non-Windows: logs are emitted to stderr.
