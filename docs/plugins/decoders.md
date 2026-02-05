# Decoders

Decoders allow translating n-bit signals into nice text representations.
They are based on the [instruction-decoder](https://github.com/ics-jku/instruction-decoder) crate.
To add additional decoders to Surfer, create a `decoders` directory in Surfer's config directory and add your decoders inside there.

| Os      | Path                                                                  |
|---------|-----------------------------------------------------------------------|
| Linux   | `~/.config/surfer/decoders/`                                        |
| Windows | `C:\Users\<Name>\AppData\Roaming\surfer-project\surfer\config\decoders\`  |
| macOS   | `/Users/<Name>/Library/Application Support/org.surfer-project.surfer/decoders/` |

To add a new decoder, create a subdirectory inside `decoders` and add the required toml files.
A decoder can consist of multiple toml files which will be merged.
You can also add project-specific decoders by creating subdirectories in `.surfer/decoders`.

The decoders show up as additional formats.

For simpler use cases, you may want to consider a [mapping](mapping) translator.
