# PURR softbuffer backports

Base: crates.io `softbuffer` 0.4.8, archive SHA-256
`aac18da81ebbf05109ab275b157c22a653bb3c12cf884450179942f81bcbf6c3`.
Upstream MIT and Apache-2.0 licenses are retained.

The backported implementation changes `src/backends/cg.rs`. Both Cargo
manifests additionally enable the `objc2-core-graphics/libc` feature needed
for the direct-provider callback API:

- Backport the retained CoreGraphics buffer and provider lock from upstream
  [#343](https://github.com/rust-windowing/softbuffer/pull/343), commit
  `cadb780d9f1a3847d9ee95ae4f82dfc10859543a`.
  Adapt its newer Pixel/AlphaMode API to 0.4.8's u32 pixels and existing
  opaque pixel format. The provider's get/release callbacks synchronize
  compositor reads with writes. Resize replaces the buffer only when its
  dimensions change; a presented buffer reports age 1.
- Backport surface layer removal on Drop from upstream
  [#355](https://github.com/rust-windowing/softbuffer/pull/355), commit
  `f0d20b9082e99db831215e12ec5ba5b2f344aae5`.

Windows and other backends are unchanged. The application uses this copy
through Cargo's `[patch.crates-io]`; do not patch the local Cargo registry.
