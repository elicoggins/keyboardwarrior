# Third-party notices

Keyboard Warrior is built on open-source libraries. The full dependency tree of
the shipped native binary is listed below, with the license each crate is
distributed under. Copyright remains with the respective authors.

## Mozilla Public License 2.0

Eleven of the crates below are licensed under the **MPL-2.0**, which requires
that recipients of a binary be able to obtain the source of those components.
They are used **unmodified**, and their complete source is available at:

| Component | Source |
| --- | --- |
| `symphonia` and its `symphonia-*` sub-crates (audio decoding) | https://github.com/pdeljanov/Symphonia |
| `option-ext` (a dependency of `dirs`) | https://github.com/soc/option-ext |

A copy of the MPL-2.0 is available at https://mozilla.org/MPL/2.0/.

## libopus

The `.opus` decoder links **libopus**, © the Xiph.Org Foundation and
contributors, distributed under the 3-clause BSD license. Source:
https://github.com/xiph/opus

## Full dependency list

| Crate | Version | License |
| --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| audiopus_sys | 0.2.2 | ISC |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bitflags | 1.3.2 | MIT/Apache-2.0 |
| block | 0.1.6 | MIT |
| bytemuck | 1.25.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| color_quant | 1.1.0 | MIT |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| coreaudio-rs | 0.11.3 | MIT/Apache-2.0 |
| coreaudio-sys | 0.2.18 | MIT |
| cpal | 0.15.3 | Apache-2.0 |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 |
| crossbeam-deque | 0.8.7 | MIT OR Apache-2.0 |
| crossbeam-epoch | 0.9.20 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.22 | MIT OR Apache-2.0 |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 |
| dirs | 5.0.1 | MIT OR Apache-2.0 |
| dirs-sys | 0.4.1 | MIT OR Apache-2.0 |
| dispatch | 0.2.0 | MIT |
| displaydoc | 0.2.6 | MIT OR Apache-2.0 |
| either | 1.16.0 | MIT OR Apache-2.0 |
| encoding_rs | 0.8.35 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| extended | 0.1.0 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.9 | MIT OR Apache-2.0 |
| foldhash | 0.1.5 | Zlib |
| fontdue | 0.9.3 | MIT OR Apache-2.0 OR Zlib |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| glam | 0.27.0 | MIT OR Apache-2.0 |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 |
| icu_collections | 2.2.0 | Unicode-3.0 |
| icu_locale_core | 2.2.0 | Unicode-3.0 |
| icu_normalizer | 2.2.0 | Unicode-3.0 |
| icu_normalizer_data | 2.2.0 | Unicode-3.0 |
| icu_properties | 2.2.0 | Unicode-3.0 |
| icu_properties_data | 2.2.0 | Unicode-3.0 |
| icu_provider | 2.2.0 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.24.9 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 |
| libc | 0.2.186 | MIT OR Apache-2.0 |
| litemap | 0.8.2 | Unicode-3.0 |
| log | 0.4.33 | MIT OR Apache-2.0 |
| mach2 | 0.4.3 | BSD-2-Clause OR MIT OR Apache-2.0 |
| macroquad | 0.4.15 | MIT OR Apache-2.0 |
| macroquad_macro | 0.1.8 | MIT/Apache-2.0 |
| malloc_buf | 0.0.6 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| midly | 0.5.3 | Unlicense |
| miniquad | 0.4.10 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| objc | 0.2.7 | MIT |
| objc-foundation | 0.1.1 | MIT |
| objc-rs | 0.2.8 | MIT |
| objc_id | 0.1.1 | MIT |
| ogg | 0.9.2 | BSD-3-Clause |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| option-ext | 0.2.0 | MPL-2.0 |
| opus | 0.3.1 | MIT/Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| png | 0.17.16 | MIT OR Apache-2.0 |
| potential_utf | 0.1.5 | Unicode-3.0 |
| proc-macro2 | 1.0.106 | MIT OR Apache-2.0 |
| quad-rand | 0.2.3 | MIT |
| quote | 1.0.46 | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| rayon | 1.12.0 | MIT OR Apache-2.0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 |
| rfd | 0.14.1 | MIT |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| rustls | 0.23.42 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.15.0 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.13 | ISC |
| serde | 1.0.228 | MIT OR Apache-2.0 |
| serde_core | 1.0.228 | MIT OR Apache-2.0 |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 |
| serde_json | 1.0.150 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| smallvec | 1.15.2 | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| subtle | 2.6.1 | BSD-3-Clause |
| symphonia | 0.5.5 | MPL-2.0 |
| symphonia-bundle-flac | 0.5.5 | MPL-2.0 |
| symphonia-bundle-mp3 | 0.5.5 | MPL-2.0 |
| symphonia-codec-pcm | 0.5.5 | MPL-2.0 |
| symphonia-codec-vorbis | 0.5.5 | MPL-2.0 |
| symphonia-core | 0.5.5 | MPL-2.0 |
| symphonia-format-ogg | 0.5.5 | MPL-2.0 |
| symphonia-format-riff | 0.5.5 | MPL-2.0 |
| symphonia-metadata | 0.5.5 | MPL-2.0 |
| symphonia-utils-xiph | 0.5.5 | MPL-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| synstructure | 0.13.2 | MIT |
| tinystr | 0.8.3 | Unicode-3.0 |
| ttf-parser | 0.21.1 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| untrusted | 0.9.0 | ISC |
| ureq | 2.12.1 | MIT OR Apache-2.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 |
| writeable | 0.6.3 | Unicode-3.0 |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.2 | Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.7 | Unicode-3.0 |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.4 | Unicode-3.0 |
| zerovec | 0.11.6 | Unicode-3.0 |
| zerovec-derive | 0.11.3 | Unicode-3.0 |
| zmij | 1.0.23 | MIT |
