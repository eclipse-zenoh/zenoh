use cfg_aliases::cfg_aliases;

fn main() {
    // these aliases should at least be included in the same aliases of Nix crate:
    // ___________________
    // |                 |
    // |  Nix aliases    |
    // |  ___________    |
    // |  |   Our   |    |
    // |  | aliases |    |
    // |  |_________|    |
    // |_________________|
    cfg_aliases! {
        dragonfly: { target_os = "dragonfly" },
        ios: { target_os = "ios" },
        freebsd: { target_os = "freebsd" },
        macos: { target_os = "macos" },
        netbsd: { target_os = "netbsd" },
        openbsd: { target_os = "openbsd" },
        watchos: { target_os = "watchos" },
        tvos: { target_os = "tvos" },
        visionos: { target_os = "visionos" },

        apple_targets: { any(ios, macos, watchos, tvos, visionos) },
        bsd: { any(freebsd, dragonfly, netbsd, openbsd, apple_targets) },
    }

    println!("cargo:rustc-check-cfg=cfg(apple_targets)");
    println!("cargo:rustc-check-cfg=cfg(bsd)");
}
