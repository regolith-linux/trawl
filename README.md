# Regolith Config Manager

Simple **Xresources** style linux based configuration system that is independent of distro / display backend (Wayland / X11 / etc).

# Build Instructions

Make sure you have the rust toolchain (version >= 1.62) installed. Use the following commands to build all the binaries:

```bash
git clone https://github.com/regolith-linux/regolith-config-manager.git
cd regolith-config-manager
# make setup command might fail. This won't effect the build.
make setup
make
```

**Note 1**: Replace `cargo build` with `cargo build --release` to build the binaries in release mode.

**Note 2**: The binaries are located in the `target/debug` directory (`target/release` if built in release mode).

**Note 3**: The `make setup` command will fail the first time you run it. This is because the code for dbus-interface is generated at this step. You can ignore this error and proceed to the following commands.

# Install Instructions

```bash
git clone https://github.com/regolith-linux/regolith-config-manager.git
cd regolith-config-manager
# make setup command might fail. This won't effect the build.
make setup
make setup
make
sudo make install
systemctl daemon-reload
systemctl enable --now trawld
```
**Note**: The `make setup` command will fail the first time you run it. This is because the code for dbus-interface is generated at this step. You can ignore this error and proceed to the following commands.


# Features

- **Xresources** like file based configuration system.
- Compatible with existing `Xresources` files.
- Display backend and distro independent.
- **Lightweight** and **Simple** to use.
- Uses **DBus session bus** to estabish IPC. This allows each user to run thier own instance of the config manager.

# Usage

Prefix the binaries with build directory path (`target/debug` or `target/release`) to use without installing.

## Resource file / Config file format

- Each line in the resource file is a key value pair separated by ':'. Eg.
  ```
  key1: value1
  key2: value2
  ```
- A valid key is an acii alphanumeric string with no whitespace, but can include the following special characters -- **' - ', ' . ', ' \_ '**. Eg.
  ```
  sway-wm.screen_timout: 100
  ```
- In case a line contains multiple colons (':'), the contents before the first colon are treated as key and the contents after the first colon are treated as value. Eg.
  ```
  swaywm.workspace.1.name: 1: Shell
  ```
- Value can be any UTF-8 string.
- Comments are prefixed with '//' and are ignored.
- By default, preprocessor directives (begin with '#') are processed by the c preprocessor. If the directive is not recognized, it is ignored.
- Preprocessor directives can be used to include other files. The directive is of the form `#include <file>`. The file is relative to the resource file.
  ```c
  #include <config.d/swayidle>
  ```
- Preprocessor directives can be used to define macros. The directive is of the form `#define <macro> <value>`. The macro is a string and the value is a string. The macro is defined in the resource file.
  ```c
  #define USERNAME "John Doe"
  ```
- Just like in C, pats of the resource file can be ignored / included ccnditioinally using the `#ifdef` and `ifndef` directives.
  ```c
  #ifdef USERNAME
  swaylock.greeter.user USERNAME // USERNAME is replaced with John Doe
  #endif
  ```

## Start the config manager (trawld)

Run `trawld` to start the config daemon. You can pass optional arguments to the daemon to customize the behavior or to change the logging level. Run `trawld --help` documentation for more information.

## Using the CLI Cleint (_trawldb_)

The CLI client allows the user to interact with the config manager. Primary functions of the client include loading configurations fromo filesRun `trawldb --help` for more information.

### Examples

1. Load a file
   ```bash
   # load a file (doesn't overwrite existing resources)
   trawldb --load example/resources
   # load a file without preprocessing
   resrdb --load example/resources --nocpp
   ```
2. Merge resources from a file
   ```bash
   # merge resource from a file (overrides existing resources)
   trawldb --merge example/resources
   # merge resource from a file without preprocessing
   trawldb --merge example/resources --nocpp
   ```
3. Save currently loaded resources into a file
   ```bash
   # if file exists it is backed up to a file with the same name but with a .bak extension
   trawldb --edit all_resources
   # specify suffix for -edit [.bak]
   trawldb --edit all_resources --backup .old
   ```
4. Query Resources
   ```bash
   # query all resources
   trawldb --query
   # query a specific resource (partial matches are also shown)
   trawldb --query key1
   ```
5. Get resource values
   ```bash
   # get the value for the resouource whose name is 'key1'
   trawldb --get key1
   ```

## Get resource value (trawlcat)

`trawlcat` is a drop-in replacement for `xrerscat` and prints the vlaue of the requested resouorce. For more info, see the ([xrerscat](https://github.com/regolith-linux/xrescat)) documentation.

## Client API (for C)

The header and implementation files for the dbus interface is autogenerated from the `service.xml` file during the `meson` build. The `client_api.h` header file provides a simple to use wrapper api. All the dbus method names are in snake case and are prefixed with `conf_client`.

## Rust Client Library

Include the `trawldb` crate in the `Cargo.toml` file. You can then use the Rust client library to interact with the config manager.

```toml
[dependencies]
trawldb = { path = "/path/to/trawldb" }
```
