# Third-party notices

Keyboard Warrior is built on open-source libraries. The full dependency tree of
the shipped native binary is listed below, with the license each crate is
distributed under. Copyright remains with the respective authors.

## Mozilla Public License 2.0

Two of the crates below (and their sub-crates) are licensed under the **MPL-2.0**, which requires
that recipients of a binary be able to obtain the source of those components.
They are used **unmodified**, and their complete source is available at:

| Component | Source |
| --- | --- |
| `symphonia` and its `symphonia-*` sub-crates (audio decoding) | https://github.com/pdeljanov/Symphonia |
| `option-ext` (a dependency of `dirs`) | https://github.com/soc/option-ext |

A copy of the MPL-2.0 is available at https://mozilla.org/MPL/2.0/.

## macroquad

macroquad is distributed under MIT OR Apache-2.0. Source:
https://github.com/not-fl3/macroquad

## libopus

The `.opus` decoder links **libopus**, © the Xiph.Org Foundation and
contributors, distributed under the 3-clause BSD license. Source:
https://github.com/xiph/opus

## Full dependency list

| Crate | Version | License | Platforms |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | all |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | all |
| alsa | 0.9.1 | Apache-2.0/MIT | Linux |
| alsa-sys | 0.3.1 | MIT | Linux |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | all |
| audiopus_sys | 0.2.2 | ISC | all |
| base64 | 0.22.1 | MIT OR Apache-2.0 | all |
| bitflags | 1.3.2 | MIT/Apache-2.0 | all |
| bitflags | 2.13.1 | MIT OR Apache-2.0 | Linux |
| bytemuck | 1.25.1 | Zlib OR Apache-2.0 OR MIT | all |
| byteorder | 1.5.0 | Unlicense OR MIT | all |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 | all |
| color_quant | 1.1.0 | MIT | all |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | macOS |
| coreaudio-rs | 0.11.3 | MIT/Apache-2.0 | macOS |
| coreaudio-sys | 0.2.18 | MIT | macOS |
| cpal | 0.15.3 | Apache-2.0 | all |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 | all |
| crossbeam-deque | 0.8.7 | MIT OR Apache-2.0 | all |
| crossbeam-epoch | 0.9.20 | MIT OR Apache-2.0 | all |
| crossbeam-utils | 0.8.22 | MIT OR Apache-2.0 | all |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 | all |
| dirs | 5.0.1 | MIT OR Apache-2.0 | all |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 | all |
| displaydoc | 0.2.6 | MIT OR Apache-2.0 | all |
| either | 1.16.0 | MIT OR Apache-2.0 | all |
| encoding_rs | 0.8.35 | (Apache-2.0 OR MIT) AND BSD-3-Clause | all |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | all |
| extended | 0.1.0 | MIT | all |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | all |
| flate2 | 1.1.9 | MIT OR Apache-2.0 | all |
| foldhash | 0.1.5 | Zlib | all |
| fontdue | 0.9.3 | MIT OR Apache-2.0 OR Zlib | all |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | all |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | all |
| glam | 0.27.0 | MIT OR Apache-2.0 | all |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | all |
| icu_collections | 2.2.0 | Unicode-3.0 | all |
| icu_locale_core | 2.2.0 | Unicode-3.0 | all |
| icu_normalizer | 2.2.0 | Unicode-3.0 | all |
| icu_normalizer_data | 2.2.0 | Unicode-3.0 | all |
| icu_properties | 2.2.0 | Unicode-3.0 | all |
| icu_properties_data | 2.2.0 | Unicode-3.0 | all |
| icu_provider | 2.2.0 | Unicode-3.0 | all |
| idna | 1.1.0 | MIT OR Apache-2.0 | all |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | all |
| image | 0.24.9 | MIT OR Apache-2.0 | all |
| itoa | 1.0.18 | MIT OR Apache-2.0 | all |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 | all |
| libc | 0.2.186 | MIT OR Apache-2.0 | macOS, Linux |
| litemap | 0.8.2 | Unicode-3.0 | all |
| log | 0.4.33 | MIT OR Apache-2.0 | all |
| mach2 | 0.4.3 | BSD-2-Clause OR MIT OR Apache-2.0 | macOS |
| macroquad | 0.4.16 | MIT OR Apache-2.0 | all |
| macroquad_macro | 0.1.8 | MIT/Apache-2.0 | all |
| malloc_buf | 0.0.6 | MIT | macOS |
| memchr | 2.8.3 | Unlicense OR MIT | all |
| midly | 0.5.3 | Unlicense | all |
| miniquad | 0.4.11 | MIT OR Apache-2.0 | all |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | all |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | all |
| objc-rs | 0.2.8 | MIT | macOS |
| ogg | 0.9.2 | BSD-3-Clause | all |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | all |
| option-ext | 0.2.0 | MPL-2.0 | all |
| opus | 0.3.1 | MIT/Apache-2.0 | all |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | all |
| png | 0.17.16 | MIT OR Apache-2.0 | all |
| potential_utf | 0.1.5 | Unicode-3.0 | all |
| proc-macro2 | 1.0.106 | MIT OR Apache-2.0 | all |
| quad-rand | 0.2.3 | MIT | all |
| quote | 1.0.46 | MIT OR Apache-2.0 | all |
| rayon | 1.12.0 | MIT OR Apache-2.0 | all |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | all |
| ring | 0.17.14 | Apache-2.0 AND ISC | all |
| rustls | 0.23.42 | Apache-2.0 OR ISC OR MIT | all |
| rustls-pki-types | 1.15.0 | MIT OR Apache-2.0 | all |
| rustls-webpki | 0.103.13 | ISC | all |
| serde | 1.0.228 | MIT OR Apache-2.0 | all |
| serde_core | 1.0.228 | MIT OR Apache-2.0 | all |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 | all |
| serde_json | 1.0.150 | MIT OR Apache-2.0 | all |
| simd-adler32 | 0.3.10 | MIT | all |
| smallvec | 1.15.2 | MIT OR Apache-2.0 | all |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | all |
| subtle | 2.6.1 | BSD-3-Clause | all |
| symphonia | 0.5.5 | MPL-2.0 | all |
| symphonia-bundle-flac | 0.5.5 | MPL-2.0 | all |
| symphonia-bundle-mp3 | 0.5.5 | MPL-2.0 | all |
| symphonia-codec-pcm | 0.5.5 | MPL-2.0 | all |
| symphonia-codec-vorbis | 0.5.5 | MPL-2.0 | all |
| symphonia-core | 0.5.5 | MPL-2.0 | all |
| symphonia-format-ogg | 0.5.5 | MPL-2.0 | all |
| symphonia-format-riff | 0.5.5 | MPL-2.0 | all |
| symphonia-metadata | 0.5.5 | MPL-2.0 | all |
| symphonia-utils-xiph | 0.5.5 | MPL-2.0 | all |
| syn | 2.0.119 | MIT OR Apache-2.0 | all |
| synstructure | 0.13.2 | MIT | all |
| tinystr | 0.8.3 | Unicode-3.0 | all |
| ttf-parser | 0.21.1 | MIT OR Apache-2.0 | all |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | all |
| untrusted | 0.9.0 | ISC | all |
| ureq | 2.12.1 | MIT OR Apache-2.0 | all |
| url | 2.5.8 | MIT OR Apache-2.0 | all |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | all |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 | all |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | all |
| winapi | 0.3.9 | MIT/Apache-2.0 | Windows |
| windows | 0.54.0 | MIT OR Apache-2.0 | Windows |
| windows-core | 0.54.0 | MIT OR Apache-2.0 | Windows |
| windows-result | 0.1.2 | MIT OR Apache-2.0 | Windows |
| windows-sys | 0.48.0 | MIT OR Apache-2.0 | Windows |
| windows-targets | 0.48.5 | MIT OR Apache-2.0 | Windows |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | Windows |
| windows_x86_64_msvc | 0.48.5 | MIT OR Apache-2.0 | Windows |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | Windows |
| writeable | 0.6.3 | Unicode-3.0 | all |
| yoke | 0.8.3 | Unicode-3.0 | all |
| yoke-derive | 0.8.2 | Unicode-3.0 | all |
| zerofrom | 0.1.8 | Unicode-3.0 | all |
| zerofrom-derive | 0.1.7 | Unicode-3.0 | all |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | all |
| zerotrie | 0.2.4 | Unicode-3.0 | all |
| zerovec | 0.11.6 | Unicode-3.0 | all |
| zerovec-derive | 0.11.3 | Unicode-3.0 | all |
| zmij | 1.0.23 | MIT | all |
