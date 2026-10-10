# Registro de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/);
versionado [semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Validado con Presto 8.8 (10-10-2026)
- **Importación de una obra real** (climatización, 208 conceptos, CI 45 %,
  descuentos y porcentajes): 52/52 partidas, 5 capítulos y PEM 157.310,88 €
  al céntimo. El fichero real no se incluye en el repositorio.
- **Exportación**: el BC3 del ejemplo abierto en Presto da 7.005,15 € y el
  mismo desglose por partidas y naturalezas.

### Corregido
- Redondeos como Presto 8.8: porcentaje = redondeo(base/100, dec. rendimiento)
  × puntos; factor × rendimiento sin redondear antes de multiplicar.
- `~C` de partidas = coste sin costes indirectos; capítulos y raíz, con ellos
  (lectura y escritura).
- BC3 que declara ANSI pero llega en UTF-8: se lee bien y se avisa; aviso si
  hay caracteres «�» (tildes perdidas antes de importar).
- Material (tipo 3) con descomposición: información, no aviso.
- CI: la comprobación DCO ignora los commits de fusión que crea GitHub.

### Añadido
- `ppto-mcp`: servidor MCP de lectura y simulación (P-020) — abrir BC3/SQLite,
  resumen, búsqueda, detalle de conceptos, recursos y horas, simulación de
  CI/precios/cantidades y de subcontratas, exportación a Excel y BC3.
  Guía en `docs/mcp.md`.
- Exportador BC3 (P-011) en el formato de Presto 8.8 (FIEBDC-3/2002, ANSI,
  CRLF) con pruebas de ida y vuelta; `--bc3 salida.bc3` en `demo` e
  `importar` y botón «Exportar BC3…».
- Texto descriptivo (`~T`) visible y editable; incluido en el Excel.
- `ppto-informes`: libro Excel con Resumen, Presupuesto (agrupado por
  capítulos), Descompuestos, Mediciones, Recursos y Horas por oficio,
  preparado para imprimir en A4. `ppto demo|importar … --excel f.xlsx` y
  botón «Excel…» en la interfaz. Cifras del motor, sin fórmulas de Excel.
- Opción «Redondear partidas que actúan como auxiliares» (Presto; desmarcada).
- Costes indirectos de obra aplicados por partida, con las opciones de
  redondeo de Presto; PEM con indirectos y coste directo por separado. Se
  leen del `~K` del BC3, se guardan en SQLite (migración 2) y se editan en
  «Resumen y venta». Recursos y subcontratas pasan a comparar con coste directo
  (`Explosion::coste_directo`, `ResultadoEscenario::coste_original/coste_escenario`).
- `ppto-bc3`: importador FIEBDC-3 (P-010) con lectura tolerante de `~V ~K ~C
  ~D ~Y ~T ~M ~N`, codificación ANSI/UTF-8, incidencias con número de línea,
  reconstrucción de la raíz, ruptura de ciclos y comparación de precios
  recalculados frente a los declarados en el BC3.
- `ppto importar <fichero.bc3> [--db …] [--todo]` y botón «Importar BC3…»
  con informe de importación en la interfaz.
- `Presupuesto::valorar`: valoración en bloque (4.000 partidas en ~70 ms).
- Líneas porcentuales con máscara FIEBDC-3 (prefijo del código antes de `%`/`&`);
  en BC3 el rendimiento del porcentaje se lee como fracción (0.03 = 3 %).
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
