# ADR-0006: Prototipo de interfaz con egui

- Estado: Propuesto
- Fecha: 2026-10-09
- Issue: P-013 (#16)

## Contexto

ADR-0001 propone Tauri para la interfaz definitiva. Antes de tener importador
BC3 se necesita una interfaz básica para que los ingenieros validen el motor
editando mediciones, rendimientos y precios. Tauri exige además Node.js y un
proyecto web, lo que encarece el primer paso para colaboradores sin ese perfil.

## Decisión

- Prototipo `crates/ppto-gui` con **egui/eframe 0.36** (Rust puro, renderizado
  OpenGL, un único ejecutable). Se compila con `cargo` sin más herramientas.
- La interfaz no calcula: toda cifra procede de `ppto-core`. Las celdas
  numéricas leen y escriben en formato español (`ppto_core::formato`).
- La elección definitiva (egui frente a Tauri) se revisa al cerrar P-013 con
  la experiencia de uso real: tablas grandes, impresión, accesibilidad.

## Consecuencias

- Coste de entrada mínimo: `cargo run -p ppto-gui`.
- egui es de modo inmediato: tablas de decenas de miles de líneas necesitarán
  virtualización (`egui_extras::TableBuilder`) cuando llegue el importador BC3.
- Si se adopta Tauri, solo se reescribe esta capa; el núcleo no cambia.
