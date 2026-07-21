# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

Please report suspected vulnerabilities privately to
**contact@abstractframework.ai**. Do not open a public issue for security reports.

Include what you can: a minimal reproducer (crafted input bytes, image, model,
or markdown), the affected API or example, and the observed behavior. You will
receive an acknowledgement, and a fix or an assessment will follow as quickly
as the issue warrants. Please allow time for a fix to ship before public
disclosure.

## Scope

AbstractTUI is a library that parses untrusted inputs by design:

- **Terminal input bytes** — the input parser consumes arbitrary byte streams
  (escape sequences, UTF-8 text, keyboard and mouse protocols).
- **Images** — PNG and JPEG decoders operate on caller-supplied files.
- **3D models** — the GLB (glTF 2.0 binary) loader operates on
  caller-supplied files.
- **Markdown** — the markdown renderer operates on caller-supplied text.

All of these parsers are bounded, reject malformed data with named errors,
and are exercised with randomized and truncated-input tests. **Any panic,
unbounded allocation, or hang triggered by crafted input is treated as a
vulnerability-class bug** — please report it, even if it looks like "just a
crash".

On the output side:

- Escape-sequence output is sanitized wherever user-provided text can reach a
  terminal control channel (window titles, desktop notifications), so
  application text cannot smuggle control sequences into the terminal. A way
  to bypass this sanitization is a valid report.
- Clipboard integration writes only, via OSC 52; the library never reads the
  clipboard.

Issues in the terminal emulators themselves are out of scope, but reports of
AbstractTUI output driving an emulator into an unsafe state are welcome.
