cfg_if::cfg_if! {
    if #[cfg(target_arch = "aarch64")] {
        #[path = "aarch64/mod.rs"]
        pub mod arch;
    } else {
        panic!("Not Supported Arch");
    }
}

pub use arch::*;
