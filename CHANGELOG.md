# Registro de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/);
versionado [semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Añadido — descuentos por fabricante
- Recursos con **PVP de tarifa y fabricante** (`PrecioTarifa`): el neto es
  PVP × (1 − descuento del fabricante). `Presupuesto::descuentos`,
  `fijar_descuento` (recalcula todos los recursos del fabricante),
  `fijar_tarifa`, `fabricantes`. Escribir el neto a mano desliga la tarifa.
- Base de datos: migración 0004 (PVP, fabricante y tabla de descuentos por
  revisión). El BC3 sigue llevando netos.
- Propuestas de la IA: líneas con `pvp` y `fabricante` y descuentos
  estimados por fabricante, que solo se usan si la obra no tiene uno.
- Interfaz: Recursos → «Descuentos por fabricante» (editar o añadir) y
  columna de tarifa con el PVP editable.

### Añadido — propuestas de la IA con aprobación (P-021)
- `ppto_core::propuesta`: propuesta de descompuesto o precio directo para
  una partida, con confianza y justificación; `aplicar_propuesta` (reutiliza
  recursos existentes con su precio, da de alta los nuevos, todo o nada) y
  `simular_propuesta`.
- `ppto-propuestas`: obra activa publicada por el programa y bandeja de
  propuestas compartida con el MCP (escritura atómica).
- MCP: `abrir_obra_activa`, `partidas_sin_precio`, `buscar_tarifas`,
  `proponer` y `ver_propuestas`. Ninguna herramienta modifica el presupuesto.
- Interfaz: «✨ Propuestas IA (n)» con vista previa del precio e importe,
  descompuesto, justificación, Aceptar/Rechazar, aceptar en bloque las de
  confianza alta y Deshacer (la propuesta vuelve a pendiente).

### Añadido — coeficiente de dificultad
- `Presupuesto::aplicar_dificultad(ámbito, coeficiente)`: multiplica los
  rendimientos de mano de obra propia de toda la obra, un capítulo o una
  partida (y de sus auxiliares). No toca el precio de la hora, materiales,
  maquinaria ni subcontratas; los % se recalculan. Partidas o auxiliares
  compartidos con otros capítulos no se tocan y se informan. Informe de
  horas y coste antes/después.
- Interfaz: pestaña Recursos → «Coeficiente de dificultad», con vista previa,
  motivo y Deshacer. Se anota en la auditoría de la revisión al guardar
  (`Almacen::anotar`).

### Añadido — modo claro / oscuro
- Interfaz: botón «☀ Claro» / «🌙 Oscuro» en la barra; la elección se guarda
  en `tema.txt` de la carpeta de configuración (sin elección, sigue al
  sistema). Colores de tipo de recurso legibles en los dos modos.

### Añadido — tarifas de subcontratas
- `ppto-tarifas`: lee la tarifa de una subcontrata en Excel (Referencia,
  Descripción, Precio y, si la hay, Unidad; proveedor y año del título).
  Sin columna de unidad, la deduce de la descripción («/ METRO», «(1H)»,
  cable, tubo…) y marca las dudosas para revisar. Biblioteca de tarifas en
  `tarifas.json` de la carpeta de configuración (nunca en el repositorio),
  búsqueda en todas y alta como recurso de subcontrata `PROVEEDOR.REFERENCIA`.
- Interfaz: «Tarifas…» (importar, revisar unidades y precios, guardar) y
  «+ Añadir línea… → De tarifa de subcontrata».
- CLI: `ppto tarifa <tarifa.xlsx>` lista los recursos con su unidad.

### Añadido — subcontrata a ratio
- `ppto_core::ratio`: `Presupuesto::aplicar_ratio(PaqueteRatio)` aplica al
  presupuesto una subcontrata por unidad de contratación (p. ej. montaje VRF
  a € por máquina): medidor = suma de las cantidades de las partidas
  elegidas; en cada partida se quitan los recursos que pone la subcontrata
  y se añade la línea de subcontrata; reparto en proporción al coste propio
  retirado (Σ rendimientos × cantidades = medidor exacto); los medios
  auxiliares se conservan. Partida entera o solo mano de obra = paquete de
  una partida. Informe: importe, retirado, coste antes/después, horas.
- Interfaz: «Subcontrata a ratio» en la pestaña Subcontratar, con vista
  previa y Deshacer.

### Añadido — presupuestos ciegos
- `ppto-ciego`: importa el listado de Presto en Excel (.xlsx/.xls) y devuelve
  el mismo .xlsx con los precios (solo se escriben las celdas PrPres;
  `fullCalcOnLoad`). Mediciones a precisión completa para cuadrar al céntimo
  con el Excel; parciales fijos; números y códigos guardados como texto;
  partidas repetidas en varios capítulos. Validado con un ciego real de 507
  partidas y 1.684 líneas de medición (no se sube).
- `traer_precios` / `Presupuesto::traer_precio`: trae de otra obra o de un BC3
  el descompuesto completo (o el precio directo) de cada partida sin precio.
- Partidas sin descomponer con precio directo (como en Presto).
- `ppto_core::formato::cant`: cantidades sin ceros sobrantes.
- CLI `ppto ciego`; interfaz: menú «📋 Ciego» y precio directo en la partida.
- Interfaz: **Deshacer / Rehacer** (Ctrl+Z, Ctrl+Y) de todos los cambios del
  presupuesto, hasta 200 pasos; navegación anterior/siguiente/sin precio
  (Ctrl+↑/↓) y buscador de partidas.

### Añadido — PDF (P-016)
- `ppto-pdf`: presupuesto en PDF con Typst incluido (ADR-0007): portada con
  logo, fecha y revisión; capítulos y subcapítulos; textos justificados con
  partición silábica en castellano; mediciones; resumen con GG, BI, IVA, total
  en letra y firma; hoja de observaciones. Plantilla editable.
- `ppto_core::formato::en_letra` (importe en letra) y `fecha_larga`/`fecha_hoy`.
- Interfaz: botón «PDF…» con ventana de opciones que recuerda empresa, firma,
  logo y observaciones. CLI: `--pdf`.
- Perfiles de empresa en el PDF (`ppto_pdf::Perfiles`): instaladora (sin PEM,
  GG ni BI) e ingeniería (PEM + GG + BI) de serie, cada uno con su logo
  (copia guardada), empresa, firma y observaciones tipo; duplicar y borrar.

### Cambiado
- Versión mínima de Rust: 1.92 (requisito de Typst).

### Añadido — edición (P-014)
- `ppto-core::edicion`: `nuevo_capitulo`, `nueva_partida`, `nuevo_recurso`,
  `anadir_linea`, `quitar_linea`, `quitar_y_purgar`, `mover_linea`,
  `borrar_concepto`, `padres`, `fijar_unidad`, `codigo_libre`. Cada operación
  deja el presupuesto válido o no lo toca.
- Interfaz: botón «Nuevo», «+ Capítulo», menú contextual en el árbol, ventana
  «Añadir línea…» con buscador o recurso nuevo, ^ v x en el descompuesto y
  unidad editable.
- Subpartidas (auxiliares descompuestos) a cualquier nivel: crear desde
  «Añadir línea…», entrar pulsando su código, «<< Volver» y ruta superior;
  `Presupuesto::ruta`.
- Arrastrar y soltar partidas en el árbol entre capítulos y niveles
  (`Presupuesto::trasladar_linea`: conserva cantidad y hoja de medición;
  si no es válido, no cambia nada).

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
