# Introduction

Surfer is a wave form viewer supporting [VCD](https://en.wikipedia.org/wiki/Value_change_dump),
[FST](https://github.com/gtkwave/libfst), and [GHW](https://ghdl.github.io/ghdl/ghw/index.html)
files as well as the memory transaction format [FTR](https://github.com/Minres/LWTR4SC).

The GHW support is not as complete as VCD and FST, but please file [issues](https://gitlab.com/surfer-project/surfer/-/issues) with examples of files not working.

It is built to be highly configurable.

## Installation

You can install Surfer either as a binary or from source. It is in general recommended to run the latest version from git.

### Installing Surfer from source

To install from source you must have a Rust compiler. To install the Rust compiler go to [https://rust-lang.org/tools/install/](https://rust-lang.org/tools/install/).

Now, you can do

``` bash
cargo install --git https://gitlab.com/surfer-project/surfer.git surfer
```

Replace `surfer` with `surver` to install the server only version.

There are a number of compile-time [features](features) that can be enabled/disabled.

If you plan to contribute to the development, please see the [development](development information).

### Installing a specific version

To install an earlier version, use:

``` bash
cargo install --locked --root <PREFIX> --git https://gitlab.com/surfer-project/surfer.git --tag v0.4.0 surfer
```

where

* `<PREFIX>` is the install location, see [`cargo install` documentation](https://doc.rust-lang.org/cargo/commands/cargo-install.html#description) for location order if `--root <PREFIX>` is ignored.
* `v0.4.0` is the tag

Note that `--locked` is important here as it will install with exactly the same dependencies that worked when releasing that version.

You may also want to replace `--tag <TAG>` with `--rev <COMMIT-HASH>` to get the version at a specific commit. For example, the version just before we broke something that we have not yet fixed.

Also note that Surfer versions are not yet that important, but more a reason to write an announcement (and to update the distributions in the next section).

### Installing Surfer as a binary

Some Linux distributions have Surfer available as a package to be installed through the package manager. These include:

* [Arch Linux (AUR)](https://aur.archlinux.org/packages/surfer-waveform-git-bin)
* [NixOS](https://search.nixos.org/packages?channel=25.05&show=surfer&query=surfer)

Homebrew also has a [formulae](https://formulae.brew.sh/formula/surfer).

In addition, it is possible to download and install the latest binary built after each merge to main:

* [Linux (x86)](https://gitlab.com/api/v4/projects/42073614/jobs/artifacts/main/raw/surfer_linux.zip?job=linux_build)
* [Rocky Linux (x86)](https://gitlab.com/api/v4/projects/42073614/jobs/artifacts/main/raw/surfer_linux_rocky.zip?job=rocky_build)
* [Linux (ARM)](https://gitlab.com/api/v4/projects/42073614/jobs/artifacts/main/raw/surfer_linux.zip?job=linux_arm64_build)
* [macOS (ARM)](https://gitlab.com/api/v4/projects/42073614/jobs/artifacts/main/raw/surfer_macos-aarch64.zip?job=macos-aarch64_build) This binary is currently not signed, so most users will not be able to install it as is. We are looking for a solution to this.
* [Windows (x86)](https://gitlab.com/api/v4/projects/42073614/jobs/artifacts/main/raw/surfer_win.zip?job=windows_build) Note that sometimes Windows Defender has been known to report Surfer [and other rust projects](https://github.com/cargo-bins/cargo-binstall/issues/945) as a trojan. If in doubt, please use [Virus total](https://www.virustotal.com/) to check.

## Starting Surfer

Once Surfer is installed, it can be started by typing `surfer` or `surfer WAVEFORMFILE.vcd` to directly load a waveform file. There are also additional arguments that can be seen by typing `surfer --help`. This should now display the arguments as:

``` text
Usage: surfer [OPTIONS] [WAVE_FILE] [COMMAND]

Commands:
  server  starts surfer in headless mode so that a user can connect to it
  help    Print this message or the help of the given subcommand(s)

Arguments:
  [WAVE_FILE]  Waveform file in VCD, FST, or GHW format

Options:
  -c, --command-file <COMMAND_FILE>  Path to a file containing 'commands' to run after a waveform has been loaded.
                                     The commands are the same as those used in the command line interface inside the program.
                                     Commands are separated by lines or ;. Empty lines are ignored. Line comments starting with
                                     `#` are supported
                                     NOTE: This feature is not permanent, it will be removed once a solid scripting system
                                     is implemented.
      --script <SCRIPT>              Alias for --command_file to support VUnit
  -s, --state-file <STATE_FILE>      Load previously saved state file
      --wcp-initiate <WCP_INITIATE>  Port for WCP to connect to
  -h, --help                         Print help
  -V, --version                      Print version
```
