# Avisos de componentes de terceros

presupuestos-bc3 se distribuye bajo GPL-3.0-or-later. Las dependencias Rust
conservan sus propias licencias; todas se comprueban en cada PR con
`cargo deny check licenses` (ver `deny.toml`).

## Fuentes tipográficas de la interfaz (`ppto-gui`)

La interfaz incluye, a través de `epaint_default_fonts` (egui), las fuentes:

| Fuente | Licencia |
|---|---|
| Ubuntu Light | Ubuntu Font Licence 1.0 |
| Hack | MIT |
| Noto Emoji | SIL Open Font License 1.1 |
| emoji-icon-font | SIL Open Font License 1.1 / MIT |

Se distribuyen sin modificar, como datos agregados al programa. Estas
licencias permiten incluirlas en software, incluido software bajo GPL, siempre
que las fuentes no se vendan por separado y se conserven sus avisos.

## Portapapeles en Windows

`clipboard-win` y `error-code` (Boost Software License 1.0), compatible con GPL.

## Maquetación PDF (`ppto-pdf`)

El PDF se compone con [Typst](https://github.com/typst/typst) (Apache-2.0),
incluido como librería. A través de `typst-assets` se incrustan las fuentes:

| Fuente | Licencia |
|---|---|
| New Computer Modern (texto del presupuesto) | GUST Font License (LPPL) |
| Libertinus Serif | SIL Open Font License 1.1 |
| DejaVu Sans Mono | Licencia Bitstream Vera / dominio público |

Se distribuyen sin modificar y los PDF generados solo incluyen el subconjunto
de glifos usado, como permiten esas licencias. `roman-numerals-rs` (0BSD).
