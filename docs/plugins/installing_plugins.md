# Installing Translator Plugins

Plugins come as a single `.wasm` file which Surfer will search for in `.surfer/translators` the current working directory, as well as in the global, OS-dependent, configuration directory

| Os      | Path                                                                               |
|---------|------------------------------------------------------------------------------------|
| Linux   | `~/.local/share/surfer/translators`                                                |
| Windows | `C:\Users\<Name>\AppData\Roaming\surfer-project\surfer\data\translators\`          |
| macOS   | `/Users/<Name>/Library/Application Support/org.surfer-project.surfer/translators/` |

To install a translator, simply put the `.wasm` file in one of these locations,
and it will be discovered automatically.

> Translators execute arbitrary code, so some care should be taken before installing translators.
> However, they are _sandboxed_ behind a web-assembly runtime that, unless there are security issues, does not allow any access to anything on the system that surfer does not allow.
>
> Currently, the only system access surfer allows for plugins is
>
> - Reading the path of the current working directory
> - **Reading** arbitrary files
