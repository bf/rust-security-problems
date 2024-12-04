// ///  <script>let val = `
// #[doc = include_str!("/etc/passwd")]
// #[doc = include_str!("/proc/self/environ")]
// /// `; document.write(val);</script>
// pub trait foo {}

// fn private_no_docs() {
//   let nobody_ever_uses_this = include_str!("/etc/passwd-FILE_DOES_NOT_EXIST");
// }

// fn main() {}

#![allow(unused_variables)]
#![debugger_visualizer(gdb_script_file = "/etc/passwd")]
