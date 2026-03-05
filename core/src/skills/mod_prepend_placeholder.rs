// This file was previously skill.rs
// Start of previous skill.rs content remains here (it was moved),
// but we need to ensure submodules are declared.

pub mod capabilities;
pub mod tool;
pub mod wasm;

// Re-export what was previously here if needed, but since this file IS the old skill.rs content,
// it should contain the code.
// Wait, I moved src/skill.rs to src/skills/mod.rs.
// So src/skills/mod.rs ALREADY contains the content of skill.rs.
// I just need to prepend the submodule declarations if they weren't there.
// skill.rs (now skills/mod.rs) already had `pub mod wasm;`.
// But it did NOT have `pub mod tool;` or `pub mod capabilities;`.
// So I need to prepend them to the moved file.
