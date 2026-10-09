# Registro de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/);
versionado [semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Añadido
- `ppto-gui`: interfaz de escritorio básica (prototipo de P-013): árbol de
  capítulos, descompuesto y mediciones editables con recálculo inmediato,
  recursos con precios editables, comparación de ofertas de subcontrata,
  resumen PEM→PEC y precio de venta por margen; abrir y guardar revisiones
  en SQLite.
- `ppto-core`: `actualizar_medicion`, `fijar_rendimiento`, `fijar_precio` y
  módulo `formato` (lectura y escritura de números en notación española).
- `ppto-db`: `listar_revisiones` y `siguiente_etiqueta`.
- `scripts/windows/`: accesos directos para abrir la interfaz y ejecutar pruebas.

## [0.1.0-alpha.1] — 2026-10-09

### Añadido
- `ppto-core`: conceptos, precios descompuestos jerárquicos con detección de
  ciclos, líneas porcentuales, mediciones con fórmulas y subtotales,
  redondeo comercial por ámbitos, explosión de recursos con descuadre de
  redondeo, margen sobre venta y recargo sobre coste, resumen PEM→PEC,
  escenarios de subcontratación por unidad o alzado.
- `ppto-db`: esquema SQLite v1, migraciones, revisiones independientes,
  bloqueo de revisiones y auditoría.
- `ppto-cli`: `ppto demo` con el caso de validación de suelo radiante.
- Documentación de arquitectura, reglas de cálculo, ADR 0001–0005 y matriz BC3.
- Gobernanza: plantillas de Issues y PR, CODEOWNERS, CI en Linux y Windows,
  comprobación de licencias.
