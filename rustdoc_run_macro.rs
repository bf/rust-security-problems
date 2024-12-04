fn private_no_docs() { 
  let nobody_ever_uses_this = include_str!("/etc/passwd-FILE_DOES_NOT_EXIST");
}

fn main() {}
