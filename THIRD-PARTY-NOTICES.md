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

## Conexión con la API (`ppto-asistente`)

HTTPS con `ureq` y `rustls` (MIT / Apache-2.0 / ISC). Los certificados se
verifican con el almacén del sistema (`rustls-platform-verifier`), que en
algunas plataformas incluye los certificados raíz de Mozilla de
`webpki-root-certs` (Community Data License Agreement Permissive 2.0: datos,
no código; ver `deny.toml`). Lectura de PDF adjuntos con `pdf-extract` (MIT).

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

## Lectura de Revit (`ppto-modelo`)

Los ficheros `.rvt` se leen con [rvt-rs](https://github.com/DrunkOnJava/rvt-rs)
(Apache-2.0, © 2026 Griffin Long), lector de ingeniería limpia del formato.
Su aviso: «"Autodesk" and "Revit" are registered trademarks of Autodesk, Inc.
This project is not affiliated with, endorsed by, or sponsored by Autodesk.»
presupuestos-bc3 tampoco está afiliado a Autodesk. Se usa una versión fijada
(v0.2.0) desde su repositorio, porque aún no está publicada en crates.io.
