# Third-party notices

OpenClip itself is licensed under the [MIT License](/license). The application bundles, links to,
or downloads the third-party components listed below. Each is the property of its respective authors
and is used under its own license. Nothing here changes the terms under which those components are
offered.

The same text is published at <https://getopenclip.pages.dev/third-party-notices>.

---

## whisper.cpp (automatic captions)

- **Component(s)**: the C/C++ inference library, statically linked into the OpenClip executable
- **Used by**: importing a video (on-device speech recognition)
- **License**: **MIT**, Copyright (c) 2023-2026 The ggml authors
- **Upstream**: <https://github.com/ggml-org/whisper.cpp>
- **Corresponding source**: the version vendored by the `whisper-rs-sys` crate, available at <https://crates.io/crates/whisper-rs-sys>
- **Why it ships**: it runs the Whisper model on your own Mac so no audio is uploaded

## ggml (tensor library inside whisper.cpp)

- **License**: **MIT**, Copyright (c) 2023-2026 The ggml authors
- **Upstream**: <https://github.com/ggml-org/ggml>
- **Used by**: whisper.cpp (statically linked; Metal backend on macOS)

## whisper-rs / whisper-rs-sys

- **License**: **Unlicense** (public domain dedication)
- **Upstream**: <https://codeberg.org/tazz4843/whisper-rs>
- **Used by**: Rust bindings that call whisper.cpp

## OpenAI Whisper model weights (downloaded on request)

- **Component(s)**: `ggml-base.en.bin`, `ggml-small.en.bin`, `ggml-large-v3-turbo-q5_0.bin`
- **Used by**: importing a video. **Not bundled.** Downloaded only when you ask for a transcript, and stored in the app data folder.
- **License**: **MIT**, Copyright (c) 2022 OpenAI. The ggml conversions are published by the whisper.cpp project.
- **Upstream**: <https://github.com/openai/whisper> and <https://huggingface.co/ggerganov/whisper.cpp>
- **Notes**: model files are fetched directly from Hugging Face to your device; OpenClip does not host or redistribute them.

---

## User interface and platform crates

- **egui / eframe**: **MIT OR Apache-2.0**. The immediate-mode GUI the app is built with. <https://github.com/emilk/egui>
- **wgpu**: **MIT OR Apache-2.0**. Graphics layer used by eframe. <https://github.com/gfx-rs/wgpu>
- **winit**: **Apache-2.0**. Windowing. <https://github.com/rust-windowing/winit>
- **muda**: **MIT OR Apache-2.0**. Native menus. <https://github.com/tauri-apps/muda>
- **rfd**: **MIT**. Native file dialogs. <https://github.com/PolyMeilex/rfd>
- **image**: **MIT OR Apache-2.0**. Image decoding. <https://github.com/image-rs/image>
- **ureq**: **MIT OR Apache-2.0**. The small HTTP client used for the model download and update check. <https://github.com/algesten/ureq>
- **serde / serde_json**: **MIT OR Apache-2.0**. Project files and the MCP protocol messages. <https://serde.rs>

Their own dependencies are licensed under permissive terms (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, Unlicense, CDLA-Permissive-2.0) or, for a few crates, the file-level Mozilla Public License 2.0. Each crate's license file ships in its published source on crates.io, and the repository's `Cargo.lock` pins the exact versions.

## Operating-system frameworks

AVFoundation, VideoToolbox, Vision, Core Image, CoreText, CoreMedia and ImageIO (macOS) are system components used through their public APIs. They are not redistributed. OpenClip bundles no H.264 or HEVC code; encoding and decoding use the operating system's licensed implementation.

## Assets created for OpenClip

The application icon and the interface artwork were created for this project and are covered by the MIT License. No third-party artwork, fonts or audio samples are bundled.

---

## License texts

### MIT License (whisper.cpp, ggml, OpenAI Whisper, and OpenClip itself)

Each work above carries its own copyright line as stated in its section. The permission notice is:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### Apache License 2.0, MPL-2.0, ISC, BSD, Zlib, Unicode-3.0, Unlicense, CDLA-Permissive-2.0

Full texts: <https://spdx.org/licenses/>.

---

To report an omission, or to request the corresponding source of any component, open an issue at
<https://github.com/MarshallNickolauson/OpenClip/issues>.
