# Third-party licences and notices

This inventory covers the 658 registry package/version entries in `Cargo.lock`. It records the licence expression declared by each exact locked crate. The grouped package inventory below is generated from the normalized `Cargo.toml` contained in each corresponding crate archive published by crates.io.

DirectPaymentTimesheets is licensed separately under `GPL-3.0-or-later`; see `LICENSE`. Third-party components remain under their own terms. An `OR` in a crate's expression permits selection of a listed alternative. For redistribution with this project, a GPL-compatible MIT or Apache-2.0 alternative is used where available. An `AND` means all listed terms apply.

## Common licence texts

The `third-party/licenses/` directory contains verbatim texts taken from exact locked crate archives:

- `Apache-2.0.txt`
- `MIT.txt`
- `BSD-2-Clause.txt`
- `BSD-3-Clause.txt`
- `Zlib.txt`
- `BSL-1.0.txt`
- `ISC.txt`
- `0BSD.txt`
- `CC0-1.0.txt`
- `Unlicense.txt`
- `Unicode-3.0.txt`
- `MPL-2.0.txt`

Copyright, attribution, warranty-disclaimer, patent, source-availability, marking and reserved-name requirements in the applicable texts must be preserved. MPL-2.0-covered files and modifications to them remain subject to MPL-2.0. This inventory does not relicense any third-party component under the project's GPL.

## Explicit upstream notices

The following verbatim notices must be preserved:

- `cfg_aliases 0.2.2`: `third-party/notices/cfg_aliases-0.2.2-NOTICES.md`
- `subsetter 0.2.6`: `third-party/notices/subsetter-0.2.6-NOTICE`
- `svg2pdf 0.13.0`: `third-party/notices/svg2pdf-0.13.0-NOTICE`

The `svg2pdf` notice also identifies licence terms for upstream code, fonts, ICC profiles and test/reference assets contained in that crate's source archive. Preserve it when redistributing those materials. A binary distributor should include only notices applicable to materials actually shipped, while a vendored-source distribution should preserve the complete upstream notice.

## egui default fonts

`epaint_default_fonts 0.33.3` declares `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. It embeds fonts used by `epaint` and `egui`. The exact supplied texts are retained under `third-party/fonts/`:

- `Hack-Regular.ttf`: `Hack-Regular.txt`, covering Source Foundry Hack, Bitstream Vera and DejaVu provenance and terms.
- `NotoEmoji-Regular.ttf`: `OFL.txt`, the SIL Open Font License 1.1.
- `Ubuntu-Light.ttf`: `UFL.txt`, the Ubuntu Font Licence 1.0.
- `emoji-icon-font.ttf`: `emoji-icon-font-mit-license.txt`.

The font files and derivatives remain under their font licences. Observe their standalone-sale, reserved-name, derivative-licensing and notice-preservation conditions. Those terms do not relicense documents rendered with the fonts or the surrounding application.

## Bundled and native components

- `rusqlite 0.31.0` enables `libsqlite3-sys 0.28.0` with bundled SQLite 3.45.0. The Rust wrappers are MIT; their exact notice is `third-party/native/libsqlite3-sys-MIT.txt`. The bundled SQLite amalgamation states that its author disclaims copyright and supplies the SQLite blessing in its source header; SQLite core is public-domain software.
- `bzip2-sys 0.1.13+1.0.8` includes bzip2 1.0.8. Its exact redistribution terms are in `third-party/native/bzip2-1.0.8-LICENSE`.
- `lzma-sys 0.1.20` includes XZ Utils 5.2. Its exact component-by-component provenance is in `third-party/native/XZ-libLZMA-COPYING`; the compiled `liblzma` code is identified there as public domain. GPL/LGPL-covered ancillary scripts, tools or build files named by that document are not application code, but their terms apply if those source materials are redistributed.
- `zstd-sys 2.0.16+zstd.1.5.7` includes Zstandard. Preserve `third-party/native/Zstandard-LICENSE` and the generated-bindings terms in `third-party/native/zstd-sys-bindings-BSD-3-Clause.txt`.
- `lettre 0.11.23` uses `native-tls 0.2.18`; the Rust wrapper notice is `third-party/native/native-tls-MIT.txt`. On Unix-like release targets this may link to the system OpenSSL implementation. The Rust `openssl 0.10.81` and `openssl-sys 0.9.117` wrapper notices are retained as `third-party/native/openssl-crate-MIT.txt` and `third-party/native/openssl-sys-MIT.txt`. A binary release must also account for the exact native TLS library actually distributed or linked on its target platform.

## Complete locked-crate inventory by declared licence expression

Each entry is `crate version`. Target-specific packages are included because they are locked even when not built on the current host.

### `MIT OR Apache-2.0` (298)
`accesskit 0.21.1`, `accesskit_atspi_common 0.14.2`, `accesskit_consumer 0.31.0`, `accesskit_macos 0.22.2`  
`accesskit_unix 0.17.2`, `accesskit_windows 0.29.2`, `aes 0.8.4`, `ahash 0.8.12`, `android-activity 0.6.1`  
`arbitrary 1.4.2`, `arboard 3.6.1`, `arrayvec 0.7.8`, `as-raw-xcb-connection 1.0.1`, `ash 0.38.0+1.3.281`  
`async-broadcast 0.7.2`, `async-recursion 1.1.1`, `async-trait 0.1.91`, `base64 0.22.1`, `base64 0.23.1`  
`bitflags 2.13.1`, `bitreader 0.3.11`, `block-buffer 0.10.4`, `block-padding 0.3.3`, `bumpalo 3.20.3`  
`bzip2 0.5.2`, `cbc 0.1.2`, `cc 1.4.0`, `cfg-if 1.0.4`, `chrono 0.4.45`, `cipher 0.4.4`  
`core-foundation 0.10.1`, `core-foundation 0.9.4`, `core-foundation-sys 0.8.7`, `core-graphics 0.23.2`  
`core-graphics-types 0.1.3`, `core-graphics-types 0.2.0`, `cpufeatures 0.2.17`, `crc 3.4.0`  
`crc-catalog 2.5.0`, `crc32fast 1.5.0`, `crossbeam-utils 0.8.22`, `crypto-common 0.1.7`, `data-url 0.3.2`  
`deranged 0.5.8`, `derive_arbitrary 1.4.2`, `digest 0.10.7`, `dirs 5.0.1`, `dirs-sys 0.4.1`  
`displaydoc 0.2.7`, `document-features 0.2.12`, `dtoa 1.0.11`, `ecolor 0.33.3`, `eframe 0.33.3`, `egui 0.33.3`  
`egui-wgpu 0.33.3`, `egui-winit 0.33.3`, `egui_glow 0.33.3`, `either 1.17.0`, `email-encoding 0.4.2`  
`emath 0.33.3`, `enumflags2 0.7.12`, `enumflags2_derive 0.7.12`, `epaint 0.33.3`, `errno 0.3.14`  
`euclid 0.22.14`, `fdeflate 0.3.7`, `find-msvc-tools 0.1.9`, `flate2 1.1.9`, `font-types 0.11.3`  
`form_urlencoded 1.2.2`, `futures-channel 0.3.33`, `futures-core 0.3.33`, `futures-io 0.3.33`  
`futures-macro 0.3.33`, `futures-task 0.3.33`, `futures-util 0.3.33`, `getrandom 0.1.16`, `getrandom 0.2.17`  
`getrandom 0.3.4`, `getrandom 0.4.3`, `gif 0.13.3`, `gif 0.14.2`, `gpu-alloc 0.6.2`, `gpu-alloc-types 0.3.1`  
`gpu-allocator 0.27.0`, `gpu-descriptor 0.3.2`, `gpu-descriptor-types 0.2.0`, `half 2.7.1`, `hashbrown 0.14.5`  
`hashbrown 0.15.5`, `hashbrown 0.16.1`, `hashbrown 0.17.1`, `hashlink 0.9.1`, `heck 0.4.1`, `hermit-abi 0.5.2`  
`hex 0.4.3`, `hmac 0.12.1`, `html5ever 0.25.2`, `httpdate 1.0.3`, `iana-time-zone 0.1.65`  
`iana-time-zone-haiku 0.1.2`, `idna 1.1.0`, `image 0.25.10`, `image-webp 0.2.4`, `inout 0.1.4`, `itoa 0.4.8`  
`itoa 1.0.18`, `jni 0.22.4`, `jni-macros 0.22.4`, `jni-sys 0.3.1`, `jni-sys 0.4.1`, `jni-sys-macros 0.4.1`  
`jobserver 0.1.35`, `js-sys 0.3.103`, `lazy_static 1.5.0`, `libc 0.2.189`, `litrs 1.0.0`, `lock_api 0.4.14`  
`log 0.4.33`, `md-5 0.10.6`, `memmap2 0.9.11`, `metal 0.32.0`, `mime 0.3.17`, `naga 27.0.3`  
`native-tls 0.2.18`, `ndk 0.9.0`, `ndk-context 0.1.1`, `ndk-sys 0.6.0+11769913`, `num-conv 0.2.2`  
`num-traits 0.2.19`, `once_cell 1.21.4`, `openssl-probe 0.2.1`, `ordered-stream 0.2.0`, `ouroboros 0.17.2`  
`ouroboros_macro 0.17.2`, `parking_lot 0.12.5`, `parking_lot_core 0.9.12`, `paste 1.0.15`  
`pathfinder_simd 0.5.6`, `pbkdf2 0.12.2`, `pdf-writer 0.12.1`, `percent-encoding 2.3.2`, `piper 0.2.5`  
`pkg-config 0.3.33`, `png 0.17.16`, `png 0.18.1`, `polycool 0.4.0`, `powerfmt 0.2.0`, `ppv-lite86 0.2.21`  
`presser 0.3.1`, `proc-macro-crate 3.5.0`, `proc-macro-error 1.0.4`, `proc-macro-error-attr 1.0.4`  
`proc-macro-hack 0.5.20+deprecated`, `proc-macro2 1.0.107`, `profiling 1.0.18`, `quote 1.0.47`, `rand 0.7.3`  
`rand 0.8.7`, `rand 0.9.5`, `rand_chacha 0.2.2`, `rand_chacha 0.9.0`, `rand_core 0.5.1`, `rand_core 0.6.4`  
`rand_core 0.9.5`, `rand_pcg 0.2.1`, `range-alloc 0.1.5`, `read-fonts 0.39.2`, `renderdoc-sys 1.1.0`  
`roxmltree 0.20.0`, `rustc_version 0.4.1`, `rustversion 1.0.23`, `scopeguard 1.2.0`  
`security-framework 3.7.0`, `security-framework-sys 2.17.0`, `semver 1.0.28`, `serde 1.0.229`  
`serde_core 1.0.229`, `serde_derive 1.0.229`, `serde_json 1.0.151`, `serde_repr 0.1.21`, `serde_spanned 0.6.9`  
`sha1 0.10.7`, `sha2 0.10.9`, `shlex 2.0.1`, `signal-hook-registry 1.4.8`, `simdutf8 0.1.5`, `skrifa 0.42.1`  
`smallvec 1.15.2`, `smol_str 0.2.2`, `socket2 0.6.5`, `stable_deref_trait 1.2.1`, `static_assertions 1.1.0`  
`string_cache 0.8.9`, `string_cache_codegen 0.5.4`, `subsetter 0.2.6`, `svg2pdf 0.13.0`, `syn 1.0.109`  
`syn 2.0.119`, `syn 3.0.3`, `tempfile 3.27.0`, `thiserror 1.0.69`, `thiserror 2.0.19`, `thiserror-impl 1.0.69`  
`thiserror-impl 2.0.19`, `time 0.3.55`, `time-core 0.1.9`, `time-macros 0.2.32`, `toml 0.8.23`  
`toml_datetime 0.6.11`, `toml_datetime 1.1.1+spec-1.1.0`, `toml_edit 0.22.27`, `toml_edit 0.25.13+spec-1.1.0`  
`toml_parser 1.1.3+spec-1.1.0`, `toml_write 0.1.2`, `ttf-parser 0.15.2`, `ttf-parser 0.25.1`, `typenum 1.20.1`  
`ucd-trie 0.1.7`, `unicode-bidi 0.3.18`, `unicode-normalization 0.1.25`, `unicode-script 0.5.8`  
`unicode-segmentation 1.13.3`, `unicode-width 0.2.2`, `url 2.5.8`, `utf-8 0.7.6`, `wasm-bindgen 0.2.126`  
`wasm-bindgen-futures 0.4.76`, `wasm-bindgen-macro 0.2.126`, `wasm-bindgen-macro-support 0.2.126`  
`wasm-bindgen-shared 0.2.126`, `web-sys 0.3.103`, `web-time 1.1.0`, `webbrowser 1.2.3`, `weezl 0.1.12`  
`wgpu 27.0.1`, `wgpu-core 27.0.3`, `wgpu-core-deps-apple 27.0.0`, `wgpu-core-deps-emscripten 27.0.0`  
`wgpu-core-deps-windows-linux-android 27.0.0`, `wgpu-hal 27.0.4`, `wgpu-types 27.0.1`, `windows 0.58.0`  
`windows 0.61.3`, `windows-collections 0.2.0`, `windows-core 0.58.0`, `windows-core 0.61.2`  
`windows-core 0.62.2`, `windows-future 0.2.1`, `windows-implement 0.58.0`, `windows-implement 0.60.2`  
`windows-interface 0.58.0`, `windows-interface 0.59.3`, `windows-link 0.1.3`, `windows-link 0.2.1`  
`windows-numerics 0.2.0`, `windows-result 0.2.0`, `windows-result 0.3.4`, `windows-result 0.4.1`  
`windows-strings 0.1.0`, `windows-strings 0.4.2`, `windows-strings 0.5.1`, `windows-sys 0.48.0`  
`windows-sys 0.52.0`, `windows-sys 0.59.0`, `windows-sys 0.60.2`, `windows-sys 0.61.2`  
`windows-targets 0.48.5`, `windows-targets 0.52.6`, `windows-targets 0.53.5`, `windows-threading 0.1.0`  
`windows_aarch64_gnullvm 0.48.5`, `windows_aarch64_gnullvm 0.52.6`, `windows_aarch64_gnullvm 0.53.1`  
`windows_aarch64_msvc 0.48.5`, `windows_aarch64_msvc 0.52.6`, `windows_aarch64_msvc 0.53.1`  
`windows_i686_gnu 0.48.5`, `windows_i686_gnu 0.52.6`, `windows_i686_gnu 0.53.1`, `windows_i686_gnullvm 0.52.6`  
`windows_i686_gnullvm 0.53.1`, `windows_i686_msvc 0.48.5`, `windows_i686_msvc 0.52.6`  
`windows_i686_msvc 0.53.1`, `windows_x86_64_gnu 0.48.5`, `windows_x86_64_gnu 0.52.6`  
`windows_x86_64_gnu 0.53.1`, `windows_x86_64_gnullvm 0.48.5`, `windows_x86_64_gnullvm 0.52.6`  
`windows_x86_64_gnullvm 0.53.1`, `windows_x86_64_msvc 0.48.5`, `windows_x86_64_msvc 0.52.6`  
`windows_x86_64_msvc 0.53.1`, `write-fonts 0.48.1`, `x11rb 0.13.2`, `x11rb-protocol 0.13.2`, `zstd-safe 7.2.4`

### `MIT` (142)
`adobe-cmap-parser 0.4.1`, `aliasable 0.1.3`, `android-properties 0.2.2`, `ashpd 0.11.1`, `block 0.1.6`  
`block2 0.5.1`, `block2 0.6.2`, `bytes 1.12.1`, `calloop 0.13.0`, `calloop 0.14.4`  
`calloop-wayland-source 0.3.0`, `calloop-wayland-source 0.4.1`, `cfg_aliases 0.2.2`, `color_quant 1.1.0`  
`combine 4.6.7`, `convert_case 0.4.0`, `core_maths 0.1.1`, `crunchy 0.2.4`, `deflate64 0.1.12`  
`derive_more 0.99.20`, `dispatch 0.2.0`, `dlib 0.5.3`, `email_address 0.2.9`, `endi 1.1.1`, `fax 0.2.7`  
`float-cmp 0.9.0`, `fontconfig-parser 0.5.8`, `fontdb 0.23.0`, `generic-array 0.14.7`, `glutin-winit 0.5.0`  
`highway 0.8.1`, `hostname 0.4.2`, `imagesize 0.13.0`, `kuchiki 0.8.1`, `lettre 0.11.23`, `libm 0.2.16`  
`libredox 0.1.18`, `libsqlite3-sys 0.28.0`, `lopdf 0.34.0`, `lopdf 0.35.0`, `lzma-rs 0.3.0`  
`malloc_buf 0.0.6`, `matches 0.1.10`, `memoffset 0.9.1`, `mio 1.2.2`, `new_debug_unreachable 1.0.6`  
`nom 7.1.3`, `nom 8.0.0`, `nom_locate 4.2.0`, `objc 0.2.7`, `objc-sys 0.3.5`, `objc2 0.5.2`, `objc2 0.6.4`  
`objc2-app-kit 0.2.2`, `objc2-cloud-kit 0.2.2`, `objc2-contacts 0.2.2`, `objc2-core-data 0.2.2`  
`objc2-core-image 0.2.2`, `objc2-core-location 0.2.2`, `objc2-encode 4.1.0`, `objc2-foundation 0.2.2`  
`objc2-foundation 0.3.2`, `objc2-link-presentation 0.2.2`, `objc2-metal 0.2.2`, `objc2-quartz-core 0.2.2`  
`objc2-symbols 0.2.2`, `objc2-ui-kit 0.2.2`, `objc2-uniform-type-identifiers 0.2.2`  
`objc2-user-notifications 0.2.2`, `openssl-sys 0.9.117`, `orbclient 0.3.55`, `ordered-float 5.3.0`  
`pdf-extract 0.7.12`, `phf 0.8.0`, `phf_codegen 0.8.0`, `phf_generator 0.11.3`, `phf_generator 0.8.0`  
`phf_macros 0.8.0`, `phf_shared 0.11.3`, `phf_shared 0.8.0`, `pico-args 0.5.0`, `pom 1.1.0`  
`precomputed-hash 0.1.1`, `printpdf 0.8.2`, `quick-xml 0.41.0`, `redox_syscall 0.4.1`, `redox_syscall 0.5.18`  
`redox_syscall 0.9.1`, `redox_users 0.4.6`, `rfd 0.15.4`, `rgb 0.8.53`, `rusqlite 0.31.0`  
`rust-fontconfig 1.2.2`, `rustybuzz 0.20.1`, `schannel 0.1.29`, `sctk-adwaita 0.10.1`, `simd-adler32 0.3.10`  
`slab 0.4.12`, `smithay-client-toolkit 0.19.2`, `smithay-client-toolkit 0.20.0`, `smithay-clipboard 0.7.3`  
`strict-num 0.1.1`, `synstructure 0.13.2`, `tiff 0.11.3`, `tokio 1.53.1`, `tokio-native-tls 0.3.1`  
`tracing 0.1.44`, `tracing-attributes 0.1.31`, `tracing-core 0.1.36`, `type1-encoding-parser 0.1.1`  
`uds_windows 1.2.1`, `urlencoding 2.1.3`, `wayland-backend 0.3.16`, `wayland-client 0.31.15`  
`wayland-csd-frame 0.3.0`, `wayland-cursor 0.31.14`, `wayland-protocols 0.32.13`  
`wayland-protocols-experimental 20250721.0.1`, `wayland-protocols-misc 0.3.12`  
`wayland-protocols-plasma 0.3.12`, `wayland-protocols-wlr 0.3.12`, `wayland-scanner 0.31.11`  
`wayland-sys 0.31.11`, `winnow 0.7.15`, `winnow 1.0.4`, `x11-dl 2.21.0`, `xcursor 0.3.11`  
`xkbcommon-dl 0.4.2`, `xml-rs 0.8.28`, `xmlwriter 0.1.0`, `zbus 5.18.0`, `zbus-lockstep 0.5.2`  
`zbus-lockstep-macros 0.5.2`, `zbus_macros 5.18.0`, `zbus_names 4.3.4`, `zbus_xml 5.2.1`, `zip 2.4.2`  
`zmij 1.0.23`, `zstd 0.13.3`, `zvariant 5.13.1`, `zvariant_derive 5.13.1`, `zvariant_utils 3.5.0`

### `Apache-2.0 OR MIT` (46)
`async-channel 2.5.0`, `async-executor 1.14.0`, `async-fs 2.2.0`, `async-io 2.6.0`, `async-lock 3.4.2`  
`async-net 2.0.0`, `async-process 2.5.0`, `async-signal 0.2.14`, `async-task 4.7.1`, `atomic-waker 1.1.2`  
`atspi 0.25.0`, `atspi-common 0.9.0`, `atspi-connection 0.9.0`, `atspi-proxies 0.9.0`, `autocfg 1.5.1`  
`bit-set 0.8.0`, `bit-vec 0.8.0`, `blocking 1.6.2`, `concurrent-queue 2.5.0`, `equivalent 1.0.2`  
`event-listener 5.4.2`, `event-listener-strategy 0.5.4`, `fastrand 2.5.0`, `futures-lite 2.6.1`  
`idna_adapter 1.2.2`, `indexmap 2.14.0`, `kurbo 0.11.3`, `kurbo 0.13.1`, `nohash-hasher 0.2.0`  
`parking 2.2.1`, `pin-project 1.1.13`, `pin-project-internal 1.1.13`, `pin-project-lite 0.2.17`  
`polling 3.11.0`, `portable-atomic 1.14.0`, `portable-atomic-util 0.2.7`, `resvg 0.45.1`, `rustc-hash 2.1.3`  
`simd_cesu8 1.2.0`, `simplecss 0.2.2`, `svgtypes 0.15.3`, `usvg 0.45.1`, `utf8_iter 1.0.4`, `uuid 1.24.0`  
`zeroize 1.9.0`, `zeroize_derive 1.5.0`

### `MIT/Apache-2.0` (43)
`android_system_properties 0.1.5`, `bitflags 1.3.2`, `bzip2-sys 0.1.13+1.0.8`, `downcast-rs 1.2.1`  
`fallible-iterator 0.3.0`, `fallible-streaming-iterator 0.1.9`, `foreign-types 0.3.2`, `foreign-types 0.5.0`  
`foreign-types-macros 0.2.4`, `foreign-types-shared 0.1.1`, `foreign-types-shared 0.3.1`, `itertools 0.10.5`  
`khronos-egl 6.0.0`, `lzma-sys 0.1.20`, `mac 0.1.1`, `minimal-lexical 0.2.1`, `mmapio 0.9.1`, `nodrop 0.1.14`  
`openssl-macros 0.1.1`, `pathfinder_geometry 0.5.1`, `plain 0.2.3`, `quick-error 2.0.1`, `rand_hc 0.2.0`  
`rangemap 1.7.1`, `roxmltree 0.14.1`, `scoped-tls 1.0.1`, `servo_arc 0.1.1`, `siphasher 0.3.11`  
`siphasher 1.0.3`, `tendril 0.4.3`, `type-map 0.5.1`, `unicode-bidi-mirroring 0.4.0`, `unicode-ccc 0.4.0`  
`unicode-properties 0.1.4`, `unicode-vo 0.1.0`, `vcpkg 0.2.15`, `version_check 0.9.5`, `winapi 0.3.9`  
`winapi-i686-pc-windows-gnu 0.4.0`, `winapi-x86_64-pc-windows-gnu 0.4.0`, `xmlparser 0.13.6`, `xz2 0.1.7`  
`zstd-sys 2.0.16+zstd.1.5.7`

### `Apache-2.0` (24)
`ab_glyph 0.2.32`, `ab_glyph_rasterizer 0.1.10`, `accesskit_winit 0.29.2`, `allsorts 0.16.1`  
`allsorts-subset-browser 0.16.0`, `codespan-reporting 0.12.0`, `gethostname 1.1.0`, `gl_generator 0.14.0`  
`glutin 0.32.3`, `glutin_egl_sys 0.7.1`, `glutin_glx_sys 0.6.1`, `glutin_wgl_sys 0.6.1`, `khronos_api 3.1.0`  
`openssl 0.10.81`, `owned_ttf_parser 0.25.1`, `spirv 0.3.0+sdk-1.3.268.0`  
`unicode-canonical-combining-class 0.5.0`, `unicode-canonical-combining-class 1.0.0`  
`unicode-general-category 0.6.0`, `unicode-general-category 1.1.0`, `unicode-joining-type 0.7.0`  
`unicode-joining-type 1.0.0`, `winit 0.30.13`, `zopfli 0.8.3`

### `Unicode-3.0` (18)
`icu_collections 2.2.0`, `icu_locale_core 2.2.0`, `icu_normalizer 2.2.0`, `icu_normalizer_data 2.2.0`  
`icu_properties 2.2.0`, `icu_properties_data 2.2.0`, `icu_provider 2.2.0`, `litemap 0.8.2`  
`potential_utf 0.1.5`, `tinystr 0.8.3`, `writeable 0.6.3`, `yoke 0.8.3`, `yoke-derive 0.8.2`, `zerofrom 0.1.8`  
`zerofrom-derive 0.1.7`, `zerotrie 0.2.4`, `zerovec 0.11.6`, `zerovec-derive 0.11.3`

### `MPL-2.0` (10)
`azul-core 0.0.5`, `azul-css 0.0.5`, `azul-layout 0.0.5`, `azul-simplecss 0.1.2`, `cssparser 0.27.2`  
`cssparser-macros 0.6.1`, `dtoa-short 0.3.5`, `option-ext 0.2.0`, `selectors 0.22.0`, `thin-slice 0.1.1`

### `MIT OR Apache-2.0 OR Zlib` (9)
`cursor-icon 1.2.0`, `glow 0.16.0`, `raw-window-handle 0.6.2`, `tinyvec_macros 0.1.1`, `xkeysym 0.2.1`  
`zune-core 0.4.12`, `zune-core 0.5.1`, `zune-jpeg 0.4.21`, `zune-jpeg 0.5.15`

### `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` (8)
`linux-raw-sys 0.12.1`, `linux-raw-sys 0.4.15`, `rustix 0.38.44`, `rustix 1.1.4`  
`wasi 0.11.1+wasi-snapshot-preview1`, `wasi 0.9.0+wasi-snapshot-preview1`, `wasip2 1.0.4+wasi-0.2.12`  
`wit-bindgen 0.57.1`

### `Zlib OR Apache-2.0 OR MIT` (8)
`bytemuck 1.25.2`, `bytemuck_derive 1.11.0`, `dispatch2 0.3.1`, `objc2-app-kit 0.3.2`  
`objc2-core-foundation 0.3.2`, `objc2-core-graphics 0.3.2`, `objc2-io-surface 0.3.2`, `tinyvec 1.12.0`

### `Apache-2.0/MIT` (6)
`bytecount 0.6.9`, `fxhash 0.2.1`, `gl-context-loader 0.1.10`, `pollster 0.4.0`, `postscript 0.14.1`  
`rustc-hash 1.1.0`

### `BSD-3-Clause` (6)
`alloc-no-stdlib 2.0.4`, `alloc-stdlib 0.2.4`, `glyph-names 0.2.0`, `subtle 2.6.1`, `tiny-skia 0.11.4`  
`tiny-skia-path 0.11.4`

### `Unlicense OR MIT` (5)
`byteorder 1.5.0`, `byteorder-lite 0.1.0`, `memchr 2.8.3`, `termcolor 1.4.1`, `winapi-util 0.1.11`

### `MIT / Apache-2.0` (4)
`cgl 0.3.2`, `euclid 0.20.14`, `futf 0.1.5`, `markup5ever 0.10.1`

### `Unlicense/MIT` (4)
`csv 1.4.0`, `csv-core 0.1.13`, `same-file 1.0.6`, `walkdir 2.5.0`

### `Zlib` (3)
`foldhash 0.1.5`, `foldhash 0.2.0`, `slotmap 1.1.1`

### `BSD-2-Clause OR Apache-2.0 OR MIT` (2)
`zerocopy 0.8.55`, `zerocopy-derive 0.8.55`

### `BSD-3-Clause OR Apache-2.0` (2)
`moxcms 0.8.1`, `pxfm 0.1.30`

### `BSD-3-Clause OR MIT OR Apache-2.0` (2)
`num_enum 0.7.6`, `num_enum_derive 0.7.6`

### `BSD-3-Clause/MIT` (2)
`brotli-decompressor 4.0.3`, `brotli-decompressor 5.0.3`

### `BSL-1.0` (2)
`clipboard-win 5.4.1`, `error-code 3.3.2`

### `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (2)
`r-efi 5.3.0`, `r-efi 6.0.0`

### `(Apache-2.0 OR MIT) AND BSD-3-Clause` (1)
`encoding_rs 0.8.35`

### `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` (1)
`epaint_default_fonts 0.33.3`

### `(MIT OR Apache-2.0) AND Unicode-3.0` (1)
`unicode-ident 1.0.24`

### `0BSD` (1)
`quoted_printable 0.5.2`

### `0BSD OR MIT OR Apache-2.0` (1)
`adler2 2.0.1`

### `Apache-2.0 AND MIT` (1)
`dpi 0.1.2`

### `Apache-2.0 OR BSL-1.0` (1)
`ryu 1.0.23`

### `BSD-2-Clause` (1)
`arrayref 0.3.9`

### `CC0-1.0` (1)
`hexf-parse 0.2.1`

### `CC0-1.0 OR MIT-0 OR Apache-2.0` (1)
`constant_time_eq 0.3.1`

### `ISC` (1)
`libloading 0.8.9`

### `MIT OR Zlib OR Apache-2.0` (1)
`miniz_oxide 0.8.9`

### `MIT OR Apache-2.0` (298)

`accesskit 0.21.1`, `accesskit_atspi_common 0.14.2`, `accesskit_consumer 0.31.0`, `accesskit_macos 0.22.2`  
`accesskit_unix 0.17.2`, `accesskit_windows 0.29.2`, `aes 0.8.4`, `ahash 0.8.12`, `android-activity 0.6.1`  
`arbitrary 1.4.2`, `arboard 3.6.1`, `arrayvec 0.7.8`, `as-raw-xcb-connection 1.0.1`, `ash 0.38.0+1.3.281`  
`async-broadcast 0.7.2`, `async-recursion 1.1.1`, `async-trait 0.1.91`, `base64 0.22.1`, `base64 0.23.1`  
`bitflags 2.13.1`, `bitreader 0.3.11`, `block-buffer 0.10.4`, `block-padding 0.3.3`, `bumpalo 3.20.3`  
`bzip2 0.5.2`, `cbc 0.1.2`, `cc 1.4.0`, `cfg-if 1.0.4`, `chrono 0.4.45`, `cipher 0.4.4`  
`core-foundation 0.10.1`, `core-foundation 0.9.4`, `core-foundation-sys 0.8.7`, `core-graphics 0.23.2`  
`core-graphics-types 0.1.3`, `core-graphics-types 0.2.0`, `cpufeatures 0.2.17`, `crc 3.4.0`  
`crc-catalog 2.5.0`, `crc32fast 1.5.0`, `crossbeam-utils 0.8.22`, `crypto-common 0.1.7`, `data-url 0.3.2`  
`deranged 0.5.8`, `derive_arbitrary 1.4.2`, `digest 0.10.7`, `dirs 5.0.1`, `dirs-sys 0.4.1`  
`displaydoc 0.2.7`, `document-features 0.2.12`, `dtoa 1.0.11`, `ecolor 0.33.3`, `eframe 0.33.3`, `egui 0.33.3`  
`egui-wgpu 0.33.3`, `egui-winit 0.33.3`, `egui_glow 0.33.3`, `either 1.17.0`, `email-encoding 0.4.2`  
`emath 0.33.3`, `enumflags2 0.7.12`, `enumflags2_derive 0.7.12`, `epaint 0.33.3`, `errno 0.3.14`  
`euclid 0.22.14`, `fdeflate 0.3.7`, `find-msvc-tools 0.1.9`, `flate2 1.1.9`, `font-types 0.11.3`  
`form_urlencoded 1.2.2`, `futures-channel 0.3.33`, `futures-core 0.3.33`, `futures-io 0.3.33`  
`futures-macro 0.3.33`, `futures-task 0.3.33`, `futures-util 0.3.33`, `getrandom 0.1.16`, `getrandom 0.2.17`  
`getrandom 0.3.4`, `getrandom 0.4.3`, `gif 0.13.3`, `gif 0.14.2`, `gpu-alloc 0.6.2`, `gpu-alloc-types 0.3.1`  
`gpu-allocator 0.27.0`, `gpu-descriptor 0.3.2`, `gpu-descriptor-types 0.2.0`, `half 2.7.1`, `hashbrown 0.14.5`  
`hashbrown 0.15.5`, `hashbrown 0.16.1`, `hashbrown 0.17.1`, `hashlink 0.9.1`, `heck 0.4.1`, `hermit-abi 0.5.2`  
`hex 0.4.3`, `hmac 0.12.1`, `html5ever 0.25.2`, `httpdate 1.0.3`, `iana-time-zone 0.1.65`  
`iana-time-zone-haiku 0.1.2`, `idna 1.1.0`, `image 0.25.10`, `image-webp 0.2.4`, `inout 0.1.4`, `itoa 0.4.8`  
`itoa 1.0.18`, `jni 0.22.4`, `jni-macros 0.22.4`, `jni-sys 0.3.1`, `jni-sys 0.4.1`, `jni-sys-macros 0.4.1`  
`jobserver 0.1.35`, `js-sys 0.3.103`, `lazy_static 1.5.0`, `libc 0.2.189`, `litrs 1.0.0`, `lock_api 0.4.14`  
`log 0.4.33`, `md-5 0.10.6`, `memmap2 0.9.11`, `metal 0.32.0`, `mime 0.3.17`, `naga 27.0.3`  
`native-tls 0.2.18`, `ndk 0.9.0`, `ndk-context 0.1.1`, `ndk-sys 0.6.0+11769913`, `num-conv 0.2.2`  
`num-traits 0.2.19`, `once_cell 1.21.4`, `openssl-probe 0.2.1`, `ordered-stream 0.2.0`, `ouroboros 0.17.2`  
`ouroboros_macro 0.17.2`, `parking_lot 0.12.5`, `parking_lot_core 0.9.12`, `paste 1.0.15`  
`pathfinder_simd 0.5.6`, `pbkdf2 0.12.2`, `pdf-writer 0.12.1`, `percent-encoding 2.3.2`, `piper 0.2.5`  
`pkg-config 0.3.33`, `png 0.17.16`, `png 0.18.1`, `polycool 0.4.0`, `powerfmt 0.2.0`, `ppv-lite86 0.2.21`  
`presser 0.3.1`, `proc-macro-crate 3.5.0`, `proc-macro-error 1.0.4`, `proc-macro-error-attr 1.0.4`  
`proc-macro-hack 0.5.20+deprecated`, `proc-macro2 1.0.107`, `profiling 1.0.18`, `quote 1.0.47`, `rand 0.7.3`  
`rand 0.8.7`, `rand 0.9.5`, `rand_chacha 0.2.2`, `rand_chacha 0.9.0`, `rand_core 0.5.1`, `rand_core 0.6.4`  
`rand_core 0.9.5`, `rand_pcg 0.2.1`, `range-alloc 0.1.5`, `read-fonts 0.39.2`, `renderdoc-sys 1.1.0`  
`roxmltree 0.20.0`, `rustc_version 0.4.1`, `rustversion 1.0.23`, `scopeguard 1.2.0`  
`security-framework 3.7.0`, `security-framework-sys 2.17.0`, `semver 1.0.28`, `serde 1.0.229`  
`serde_core 1.0.229`, `serde_derive 1.0.229`, `serde_json 1.0.151`, `serde_repr 0.1.21`, `serde_spanned 0.6.9`  
`sha1 0.10.7`, `sha2 0.10.9`, `shlex 2.0.1`, `signal-hook-registry 1.4.8`, `simdutf8 0.1.5`, `skrifa 0.42.1`  
`smallvec 1.15.2`, `smol_str 0.2.2`, `socket2 0.6.5`, `stable_deref_trait 1.2.1`, `static_assertions 1.1.0`  
`string_cache 0.8.9`, `string_cache_codegen 0.5.4`, `subsetter 0.2.6`, `svg2pdf 0.13.0`, `syn 1.0.109`  
`syn 2.0.119`, `syn 3.0.3`, `tempfile 3.27.0`, `thiserror 1.0.69`, `thiserror 2.0.19`, `thiserror-impl 1.0.69`  
`thiserror-impl 2.0.19`, `time 0.3.55`, `time-core 0.1.9`, `time-macros 0.2.32`, `toml 0.8.23`  
`toml_datetime 0.6.11`, `toml_datetime 1.1.1+spec-1.1.0`, `toml_edit 0.22.27`, `toml_edit 0.25.13+spec-1.1.0`  
`toml_parser 1.1.3+spec-1.1.0`, `toml_write 0.1.2`, `ttf-parser 0.15.2`, `ttf-parser 0.25.1`, `typenum 1.20.1`  
`ucd-trie 0.1.7`, `unicode-bidi 0.3.18`, `unicode-normalization 0.1.25`, `unicode-script 0.5.8`  
`unicode-segmentation 1.13.3`, `unicode-width 0.2.2`, `url 2.5.8`, `utf-8 0.7.6`, `wasm-bindgen 0.2.126`  
`wasm-bindgen-futures 0.4.76`, `wasm-bindgen-macro 0.2.126`, `wasm-bindgen-macro-support 0.2.126`  
`wasm-bindgen-shared 0.2.126`, `web-sys 0.3.103`, `web-time 1.1.0`, `webbrowser 1.2.3`, `weezl 0.1.12`  
`wgpu 27.0.1`, `wgpu-core 27.0.3`, `wgpu-core-deps-apple 27.0.0`, `wgpu-core-deps-emscripten 27.0.0`  
`wgpu-core-deps-windows-linux-android 27.0.0`, `wgpu-hal 27.0.4`, `wgpu-types 27.0.1`, `windows 0.58.0`  
`windows 0.61.3`, `windows-collections 0.2.0`, `windows-core 0.58.0`, `windows-core 0.61.2`  
`windows-core 0.62.2`, `windows-future 0.2.1`, `windows-implement 0.58.0`, `windows-implement 0.60.2`  
`windows-interface 0.58.0`, `windows-interface 0.59.3`, `windows-link 0.1.3`, `windows-link 0.2.1`  
`windows-numerics 0.2.0`, `windows-result 0.2.0`, `windows-result 0.3.4`, `windows-result 0.4.1`  
`windows-strings 0.1.0`, `windows-strings 0.4.2`, `windows-strings 0.5.1`, `windows-sys 0.48.0`  
`windows-sys 0.52.0`, `windows-sys 0.59.0`, `windows-sys 0.60.2`, `windows-sys 0.61.2`  
`windows-targets 0.48.5`, `windows-targets 0.52.6`, `windows-targets 0.53.5`, `windows-threading 0.1.0`  
`windows_aarch64_gnullvm 0.48.5`, `windows_aarch64_gnullvm 0.52.6`, `windows_aarch64_gnullvm 0.53.1`  
`windows_aarch64_msvc 0.48.5`, `windows_aarch64_msvc 0.52.6`, `windows_aarch64_msvc 0.53.1`  
`windows_i686_gnu 0.48.5`, `windows_i686_gnu 0.52.6`, `windows_i686_gnu 0.53.1`, `windows_i686_gnullvm 0.52.6`  
`windows_i686_gnullvm 0.53.1`, `windows_i686_msvc 0.48.5`, `windows_i686_msvc 0.52.6`  
`windows_i686_msvc 0.53.1`, `windows_x86_64_gnu 0.48.5`, `windows_x86_64_gnu 0.52.6`  
`windows_x86_64_gnu 0.53.1`, `windows_x86_64_gnullvm 0.48.5`, `windows_x86_64_gnullvm 0.52.6`  
`windows_x86_64_gnullvm 0.53.1`, `windows_x86_64_msvc 0.48.5`, `windows_x86_64_msvc 0.52.6`  
`windows_x86_64_msvc 0.53.1`, `write-fonts 0.48.1`, `x11rb 0.13.2`, `x11rb-protocol 0.13.2`, `zstd-safe 7.2.4`

### `MIT` (142)

`adobe-cmap-parser 0.4.1`, `aliasable 0.1.3`, `android-properties 0.2.2`, `ashpd 0.11.1`, `block 0.1.6`  
`block2 0.5.1`, `block2 0.6.2`, `bytes 1.12.1`, `calloop 0.13.0`, `calloop 0.14.4`  
`calloop-wayland-source 0.3.0`, `calloop-wayland-source 0.4.1`, `cfg_aliases 0.2.2`, `color_quant 1.1.0`  
`combine 4.6.7`, `convert_case 0.4.0`, `core_maths 0.1.1`, `crunchy 0.2.4`, `deflate64 0.1.12`  
`derive_more 0.99.20`, `dispatch 0.2.0`, `dlib 0.5.3`, `email_address 0.2.9`, `endi 1.1.1`, `fax 0.2.7`  
`float-cmp 0.9.0`, `fontconfig-parser 0.5.8`, `fontdb 0.23.0`, `generic-array 0.14.7`, `glutin-winit 0.5.0`  
`highway 0.8.1`, `hostname 0.4.2`, `imagesize 0.13.0`, `kuchiki 0.8.1`, `lettre 0.11.23`, `libm 0.2.16`  
`libredox 0.1.18`, `libsqlite3-sys 0.28.0`, `lopdf 0.34.0`, `lopdf 0.35.0`, `lzma-rs 0.3.0`  
`malloc_buf 0.0.6`, `matches 0.1.10`, `memoffset 0.9.1`, `mio 1.2.2`, `new_debug_unreachable 1.0.6`  
`nom 7.1.3`, `nom 8.0.0`, `nom_locate 4.2.0`, `objc 0.2.7`, `objc-sys 0.3.5`, `objc2 0.5.2`, `objc2 0.6.4`  
`objc2-app-kit 0.2.2`, `objc2-cloud-kit 0.2.2`, `objc2-contacts 0.2.2`, `objc2-core-data 0.2.2`  
`objc2-core-image 0.2.2`, `objc2-core-location 0.2.2`, `objc2-encode 4.1.0`, `objc2-foundation 0.2.2`  
`objc2-foundation 0.3.2`, `objc2-link-presentation 0.2.2`, `objc2-metal 0.2.2`, `objc2-quartz-core 0.2.2`  
`objc2-symbols 0.2.2`, `objc2-ui-kit 0.2.2`, `objc2-uniform-type-identifiers 0.2.2`  
`objc2-user-notifications 0.2.2`, `openssl-sys 0.9.117`, `orbclient 0.3.55`, `ordered-float 5.3.0`  
`pdf-extract 0.7.12`, `phf 0.8.0`, `phf_codegen 0.8.0`, `phf_generator 0.11.3`, `phf_generator 0.8.0`  
`phf_macros 0.8.0`, `phf_shared 0.11.3`, `phf_shared 0.8.0`, `pico-args 0.5.0`, `pom 1.1.0`  
`precomputed-hash 0.1.1`, `printpdf 0.8.2`, `quick-xml 0.41.0`, `redox_syscall 0.4.1`, `redox_syscall 0.5.18`  
`redox_syscall 0.9.1`, `redox_users 0.4.6`, `rfd 0.15.4`, `rgb 0.8.53`, `rusqlite 0.31.0`  
`rust-fontconfig 1.2.2`, `rustybuzz 0.20.1`, `schannel 0.1.29`, `sctk-adwaita 0.10.1`, `simd-adler32 0.3.10`  
`slab 0.4.12`, `smithay-client-toolkit 0.19.2`, `smithay-client-toolkit 0.20.0`, `smithay-clipboard 0.7.3`  
`strict-num 0.1.1`, `synstructure 0.13.2`, `tiff 0.11.3`, `tokio 1.53.1`, `tokio-native-tls 0.3.1`  
`tracing 0.1.44`, `tracing-attributes 0.1.31`, `tracing-core 0.1.36`, `type1-encoding-parser 0.1.1`  
`uds_windows 1.2.1`, `urlencoding 2.1.3`, `wayland-backend 0.3.16`, `wayland-client 0.31.15`  
`wayland-csd-frame 0.3.0`, `wayland-cursor 0.31.14`, `wayland-protocols 0.32.13`  
`wayland-protocols-experimental 20250721.0.1`, `wayland-protocols-misc 0.3.12`  
`wayland-protocols-plasma 0.3.12`, `wayland-protocols-wlr 0.3.12`, `wayland-scanner 0.31.11`  
`wayland-sys 0.31.11`, `winnow 0.7.15`, `winnow 1.0.4`, `x11-dl 2.21.0`, `xcursor 0.3.11`  
`xkbcommon-dl 0.4.2`, `xml-rs 0.8.28`, `xmlwriter 0.1.0`, `zbus 5.18.0`, `zbus-lockstep 0.5.2`  
`zbus-lockstep-macros 0.5.2`, `zbus_macros 5.18.0`, `zbus_names 4.3.4`, `zbus_xml 5.2.1`, `zip 2.4.2`  
`zmij 1.0.23`, `zstd 0.13.3`, `zvariant 5.13.1`, `zvariant_derive 5.13.1`, `zvariant_utils 3.5.0`

### `Apache-2.0 OR MIT` (46)

`async-channel 2.5.0`, `async-executor 1.14.0`, `async-fs 2.2.0`, `async-io 2.6.0`, `async-lock 3.4.2`  
`async-net 2.0.0`, `async-process 2.5.0`, `async-signal 0.2.14`, `async-task 4.7.1`, `atomic-waker 1.1.2`  
`atspi 0.25.0`, `atspi-common 0.9.0`, `atspi-connection 0.9.0`, `atspi-proxies 0.9.0`, `autocfg 1.5.1`  
`bit-set 0.8.0`, `bit-vec 0.8.0`, `blocking 1.6.2`, `concurrent-queue 2.5.0`, `equivalent 1.0.2`  
`event-listener 5.4.2`, `event-listener-strategy 0.5.4`, `fastrand 2.5.0`, `futures-lite 2.6.1`  
`idna_adapter 1.2.2`, `indexmap 2.14.0`, `kurbo 0.11.3`, `kurbo 0.13.1`, `nohash-hasher 0.2.0`  
`parking 2.2.1`, `pin-project 1.1.13`, `pin-project-internal 1.1.13`, `pin-project-lite 0.2.17`  
`polling 3.11.0`, `portable-atomic 1.14.0`, `portable-atomic-util 0.2.7`, `resvg 0.45.1`, `rustc-hash 2.1.3`  
`simd_cesu8 1.2.0`, `simplecss 0.2.2`, `svgtypes 0.15.3`, `usvg 0.45.1`, `utf8_iter 1.0.4`, `uuid 1.24.0`  
`zeroize 1.9.0`, `zeroize_derive 1.5.0`

### `MIT/Apache-2.0` (43)

`android_system_properties 0.1.5`, `bitflags 1.3.2`, `bzip2-sys 0.1.13+1.0.8`, `downcast-rs 1.2.1`  
`fallible-iterator 0.3.0`, `fallible-streaming-iterator 0.1.9`, `foreign-types 0.3.2`, `foreign-types 0.5.0`  
`foreign-types-macros 0.2.4`, `foreign-types-shared 0.1.1`, `foreign-types-shared 0.3.1`, `itertools 0.10.5`  
`khronos-egl 6.0.0`, `lzma-sys 0.1.20`, `mac 0.1.1`, `minimal-lexical 0.2.1`, `mmapio 0.9.1`, `nodrop 0.1.14`  
`openssl-macros 0.1.1`, `pathfinder_geometry 0.5.1`, `plain 0.2.3`, `quick-error 2.0.1`, `rand_hc 0.2.0`  
`rangemap 1.7.1`, `roxmltree 0.14.1`, `scoped-tls 1.0.1`, `servo_arc 0.1.1`, `siphasher 0.3.11`  
`siphasher 1.0.3`, `tendril 0.4.3`, `type-map 0.5.1`, `unicode-bidi-mirroring 0.4.0`, `unicode-ccc 0.4.0`  
`unicode-properties 0.1.4`, `unicode-vo 0.1.0`, `vcpkg 0.2.15`, `version_check 0.9.5`, `winapi 0.3.9`  
`winapi-i686-pc-windows-gnu 0.4.0`, `winapi-x86_64-pc-windows-gnu 0.4.0`, `xmlparser 0.13.6`, `xz2 0.1.7`  
`zstd-sys 2.0.16+zstd.1.5.7`

### `Apache-2.0` (24)

`ab_glyph 0.2.32`, `ab_glyph_rasterizer 0.1.10`, `accesskit_winit 0.29.2`, `allsorts 0.16.1`  
`allsorts-subset-browser 0.16.0`, `codespan-reporting 0.12.0`, `gethostname 1.1.0`, `gl_generator 0.14.0`  
`glutin 0.32.3`, `glutin_egl_sys 0.7.1`, `glutin_glx_sys 0.6.1`, `glutin_wgl_sys 0.6.1`, `khronos_api 3.1.0`  
`openssl 0.10.81`, `owned_ttf_parser 0.25.1`, `spirv 0.3.0+sdk-1.3.268.0`  
`unicode-canonical-combining-class 0.5.0`, `unicode-canonical-combining-class 1.0.0`  
`unicode-general-category 0.6.0`, `unicode-general-category 1.1.0`, `unicode-joining-type 0.7.0`  
`unicode-joining-type 1.0.0`, `winit 0.30.13`, `zopfli 0.8.3`

### `Unicode-3.0` (18)

`icu_collections 2.2.0`, `icu_locale_core 2.2.0`, `icu_normalizer 2.2.0`, `icu_normalizer_data 2.2.0`  
`icu_properties 2.2.0`, `icu_properties_data 2.2.0`, `icu_provider 2.2.0`, `litemap 0.8.2`  
`potential_utf 0.1.5`, `tinystr 0.8.3`, `writeable 0.6.3`, `yoke 0.8.3`, `yoke-derive 0.8.2`, `zerofrom 0.1.8`  
`zerofrom-derive 0.1.7`, `zerotrie 0.2.4`, `zerovec 0.11.6`, `zerovec-derive 0.11.3`

### `MPL-2.0` (10)

`azul-core 0.0.5`, `azul-css 0.0.5`, `azul-layout 0.0.5`, `azul-simplecss 0.1.2`, `cssparser 0.27.2`  
`cssparser-macros 0.6.1`, `dtoa-short 0.3.5`, `option-ext 0.2.0`, `selectors 0.22.0`, `thin-slice 0.1.1`

### `MIT OR Apache-2.0 OR Zlib` (9)

`cursor-icon 1.2.0`, `glow 0.16.0`, `raw-window-handle 0.6.2`, `tinyvec_macros 0.1.1`, `xkeysym 0.2.1`  
`zune-core 0.4.12`, `zune-core 0.5.1`, `zune-jpeg 0.4.21`, `zune-jpeg 0.5.15`

### `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` (8)

`linux-raw-sys 0.12.1`, `linux-raw-sys 0.4.15`, `rustix 0.38.44`, `rustix 1.1.4`  
`wasi 0.11.1+wasi-snapshot-preview1`, `wasi 0.9.0+wasi-snapshot-preview1`, `wasip2 1.0.4+wasi-0.2.12`  
`wit-bindgen 0.57.1`

### `Zlib OR Apache-2.0 OR MIT` (8)

`bytemuck 1.25.2`, `bytemuck_derive 1.11.0`, `dispatch2 0.3.1`, `objc2-app-kit 0.3.2`  
`objc2-core-foundation 0.3.2`, `objc2-core-graphics 0.3.2`, `objc2-io-surface 0.3.2`, `tinyvec 1.12.0`

### `Apache-2.0/MIT` (6)

`bytecount 0.6.9`, `fxhash 0.2.1`, `gl-context-loader 0.1.10`, `pollster 0.4.0`, `postscript 0.14.1`  
`rustc-hash 1.1.0`

### `BSD-3-Clause` (6)

`alloc-no-stdlib 2.0.4`, `alloc-stdlib 0.2.4`, `glyph-names 0.2.0`, `subtle 2.6.1`, `tiny-skia 0.11.4`  
`tiny-skia-path 0.11.4`

### `Unlicense OR MIT` (5)

`byteorder 1.5.0`, `byteorder-lite 0.1.0`, `memchr 2.8.3`, `termcolor 1.4.1`, `winapi-util 0.1.11`

### `MIT / Apache-2.0` (4)

`cgl 0.3.2`, `euclid 0.20.14`, `futf 0.1.5`, `markup5ever 0.10.1`

### `Unlicense/MIT` (4)

`csv 1.4.0`, `csv-core 0.1.13`, `same-file 1.0.6`, `walkdir 2.5.0`

### `Zlib` (3)

`foldhash 0.1.5`, `foldhash 0.2.0`, `slotmap 1.1.1`

### `BSD-2-Clause OR Apache-2.0 OR MIT` (2)

`zerocopy 0.8.55`, `zerocopy-derive 0.8.55`

### `BSD-3-Clause OR Apache-2.0` (2)

`moxcms 0.8.1`, `pxfm 0.1.30`

### `BSD-3-Clause OR MIT OR Apache-2.0` (2)

`num_enum 0.7.6`, `num_enum_derive 0.7.6`

### `BSD-3-Clause/MIT` (2)

`brotli-decompressor 4.0.3`, `brotli-decompressor 5.0.3`

### `BSL-1.0` (2)

`clipboard-win 5.4.1`, `error-code 3.3.2`

### `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (2)

`r-efi 5.3.0`, `r-efi 6.0.0`

### `(Apache-2.0 OR MIT) AND BSD-3-Clause` (1)

`encoding_rs 0.8.35`

### `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` (1)

`epaint_default_fonts 0.33.3`

### `(MIT OR Apache-2.0) AND Unicode-3.0` (1)

`unicode-ident 1.0.24`

### `0BSD` (1)

`quoted_printable 0.5.2`

### `0BSD OR MIT OR Apache-2.0` (1)

`adler2 2.0.1`

### `Apache-2.0 AND MIT` (1)

`dpi 0.1.2`

### `Apache-2.0 OR BSL-1.0` (1)

`ryu 1.0.23`

### `BSD-2-Clause` (1)

`arrayref 0.3.9`

### `CC0-1.0` (1)

`hexf-parse 0.2.1`

### `CC0-1.0 OR MIT-0 OR Apache-2.0` (1)

`constant_time_eq 0.3.1`

### `ISC` (1)

`libloading 0.8.9`

### `MIT OR Zlib OR Apache-2.0` (1)

`miniz_oxide 0.8.9`

