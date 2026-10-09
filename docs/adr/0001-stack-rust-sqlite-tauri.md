# ADR-0001: Rust + SQLite + Tauri

- Estado: Propuesto
- Fecha: 2026-10-09
- Issue: P-005

## Contexto

El pliego fija Rust, SQLite, escritorio y MCP. Hay que concretar cómo se
reparten responsabilidades para que ingenieros sin experiencia en Rust puedan
contribuir validando cálculos y para que el motor sea reutilizable por la
interfaz, el servidor MCP y la línea de órdenes.

## Decisión

- *Cargo workspace* con crates de responsabilidad única. `ppto-core` es puro
  (sin E/S, sin dependencias de plataforma) y concentra todo el cálculo.
- SQLite vía `rusqlite` con la biblioteca incluida (`bundled`): un fichero por
  obra o cartera, sin servidor, copiable y auditable. Migraciones con
  `PRAGMA user_version` y SQL plano versionado en el repositorio.
- Interfaz de escritorio con **Tauri 2** (núcleo Rust + webview), instalador
  MSI/NSIS para Windows. Se descarta Qt por licencia LGPL de enlace dinámico
  y por coste de empaquetado; se descarta Electron por peso.
- MCP como crate propio que reutiliza `ppto-core` y `ppto-db`.

## Consecuencias

- Las pruebas del motor corren en milisegundos y en cualquier SO.
- La interfaz web de Tauri permite tablas editables con bibliotecas maduras.
- Hay que mantener la frontera: ningún crate de UI o MCP calcula importes.
