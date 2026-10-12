# presupuestos-bc3

Gestor libre de **presupuestos de construcción e instalaciones** para el
mercado español, con intercambio **BC3 / FIEBDC-3**, motor económico
verificable, control de costes y subcontratación, informes configurables e
integración **MCP**.

> **Estado: prototipo (v0.1.0-alpha.1).** Motor económico, persistencia
> SQLite, interfaz básica, importación y exportación BC3 e informe Excel
> funcionando, y servidor MCP de consulta. **BC3 validado con Presto 8.8 en
> ambos sentidos** (obra real: PEM al céntimo). Edición completa en la
> interfaz, informes PDF y cambios vía MCP en desarrollo.
> No usar todavía para presupuestos reales.

## Qué hace hoy

- Precios descompuestos jerárquicos (materiales, mano de obra, maquinaria,
  subcontratas, porcentajes y precios auxiliares) con detección de ciclos.
- Mediciones con dimensiones, deducciones, subtotales y fórmulas
  (`a*p*b^2/4*d`), con aviso de incoherencias de unidades.
- Aritmética decimal exacta y redondeo comercial con decimales por ámbito.
- Explosión de recursos (horas por oficio, materiales, subcontratas) con
  **descuadre de redondeo** explícito frente al PEM.
- Margen sobre venta frente a recargo sobre coste; resumen PEM → GG → BI → IVA.
- **Sustitución de mano de obra propia por subcontrata** por m², ml, ud o
  alzado, sin tocar el presupuesto original y sin duplicar costes.
- SQLite con migraciones, revisiones independientes (R2 no altera R1),
  revisiones bloqueables y auditoría.

```console
$ cargo run -p ppto-cli -- demo
...
CAPÍTULO C01 Calefacción por suelo radiante
  SR.M2    m2       210,00 ×     29,89 =    6.276,90
  SR.COL8  ud         3,00 ×    242,75 =      728,25
  PEM                                     7.005,15
...
ESCENARIOS DE SUBCONTRATACIÓN DEL MONTAJE
  SUB-A Montaje por m² de superficie radiante  contratado  1.890,00 (  9,00 €/m²)  PEM  6.505,35 ...
```

Cada cifra está calculada a mano en
[`docs/casos-validacion/suelo-radiante.md`](docs/casos-validacion/suelo-radiante.md).

## Importar un BC3

```console
cargo run -p ppto-cli -- importar presupuesto.bc3 --db presupuesto.sqlite
```

Lee el BC3 aunque tenga defectos, lista cada incidencia con su línea y
recalcula todos los precios para señalar los que **no cuadran** con lo que
declara el fichero. También desde la interfaz: botón «Importar BC3…».
Criterios de interpretación en [`docs/bc3/matriz-registros.md`](docs/bc3/matriz-registros.md).

## Exportar a BC3

```console
cargo run -p ppto-cli -- importar presupuesto.bc3 --bc3 copia.bc3
```

Escribe FIEBDC-3/2002 en ANSI, como Presto 8.8. También con «Exportar BC3…».

## Informe Excel

```console
cargo run -p ppto-cli -- importar presupuesto.bc3 --excel presupuesto.xlsx
```

Seis hojas: resumen por capítulos (coste directo, indirectos, PEM, GG, BI,
IVA), presupuesto, descompuestos, mediciones, recursos y horas por oficio.
También con el botón «Excel…» de la interfaz.

## Presupuestos ciegos en Excel

El listado «Presupuesto y mediciones» de Presto exportado a Excel (columnas
Código, Nat, Ud, Resumen, Comentario, N, Longitud, Anchura, Altura, Parcial,
CanPres, PrPres, ImpPres) con los precios a cero:

1. **📋 Ciego → Abrir Excel ciego…** lo importa: capítulos y subcapítulos,
   partidas con su texto y sus líneas de medición, tal como las calcula el
   Excel (dimensión 0 = 1; parciales fijos de líneas con fórmula).
2. Se valora: **precio directo** en la partida, descomponiéndola, o
   **Traer precios de un BC3…** (base de precios, obra anterior o el mismo
   ciego valorado en Presto: trae el descompuesto completo).
3. **Devolver Excel valorado…** guarda **el mismo fichero del cliente** con
   los precios en la columna PrPres. No se toca nada más (formato, fórmulas,
   textos) y Excel recalcula al abrir. El total del programa coincide al
   céntimo con el del Excel.

```console
cargo run -p ppto-cli -- ciego ciego.xlsx --precios base.bc3 --valorado ciego-valorado.xlsx
cargo run -p ppto-cli -- ciego ciego.xlsx --bc3 ciego.bc3     # para valorarlo en Presto
```

## Tarifas de subcontratas

Las tarifas de las subcontratas (p. ej. «Instalación detector analógico
15,00 €/ud», «Cableado manguera 4,50 €/m») se importan desde Excel en
**Tarifas…**: columnas Referencia, Descripción, Precio y, si existe,
Unidad. Si la tarifa no trae unidad se deduce de la descripción y las dudosas
salen en naranja para revisarlas. Se guardan solo en el equipo del usuario
(`%APPDATA%\presupuestos-bc3\tarifas.json`).

En un descompuesto, **+ Añadir línea… → De tarifa de subcontrata** busca en
todas las tarifas y añade el recurso como subcontrata con código
`PROVEEDOR.REFERENCIA`, en lugar de la mano de obra propia.

```
ppto tarifa "Tarifa proveedor 2026.xlsx"     # comprobar lo que se importa
```

## Presupuesto en PDF

Botón **PDF…** de la interfaz (o `--pdf presupuesto.pdf` en `ppto demo` e
`ppto importar`). Maquetado con Typst, incluido en el programa:

- **Portada** con logo (PNG, JPG o SVG), tipo de documento, título, cliente,
  referencia, **revisión**, **fecha** e importe total.
- Capítulos y subcapítulos; cada partida con su texto **justificado y con
  partición silábica en castellano**, líneas de medición, cantidad, precio e
  importe.
- **Resumen** por capítulos con %, PEM, GG, BI, IVA, total y total en letra,
  lugar, fecha y firma.
- Hoja final de **observaciones al presupuesto** (editables; se proponen las
  habituales).

**Perfiles de empresa.** Cada perfil guarda su logo (se sube un PNG, JPG o
SVG y se conserva una copia), empresa, firma, observaciones y tipo de resumen.
Vienen dos de serie, y se pueden duplicar, renombrar y borrar:

| Perfil | Resumen | Observaciones |
|---|---|---|
| **Instaladora** («Oferta») | Base imponible, IVA y total (sin PEM, GG ni BI) | Validez, alcance, exclusiones, garantía, pago |
| **Ingeniería** («Presupuesto») | PEM, 13 % GG, 6 % BI, IVA y total | Validez, mediciones, exclusiones, plazos, pago |

Se guardan en `%APPDATA%\presupuestos-bc3\perfiles.json`. El diseño está en `crates/ppto-pdf/plantillas/presupuesto.typ`:
«Guardar copia de la plantilla…» la deja en una carpeta para retocarla y
«Usar plantilla…» la aplica.

## Claude Desktop (MCP)

`cargo build --release -p ppto-mcp` y añadir `target\release\ppto-mcp.exe` a
la configuración de Claude Desktop. Permite preguntar por el presupuesto y
simular cambios y subcontratas sin modificarlo. Ver [`docs/mcp.md`](docs/mcp.md).

### Propuestas de la IA con aprobación

La IA (Claude u otra compatible con MCP) **nunca modifica el presupuesto**:
propone y usted decide.

1. Abra la obra en el programa (p. ej. un ciego recién importado).
2. Pida a la IA: «abre la obra activa y propón precio a las partidas sin
   precio». Usa `abrir_obra_activa`, `partidas_sin_precio`, `buscar`,
   `ver_concepto` y `buscar_tarifas` para reutilizar sus recursos, precios y
   tarifas de subcontratas, y deja una propuesta por partida con `proponer`
   (descompuesto o precio directo, con confianza y justificación).
3. En el programa, **✨ Propuestas IA (n)** muestra cada propuesta con el
   precio actual y el propuesto, el importe, el descompuesto (recursos
   nuevos marcados) y la justificación: **Aceptar** o **Rechazar**, una a
   una o todas las de confianza alta. Aceptar se deshace con Ctrl+Z.

Programa y MCP se comunican por dos ficheros de la carpeta de configuración
(`obra-activa.json` y `propuestas.json`).

## Interfaz de escritorio (prototipo)

```console
cargo run --release -p ppto-gui
```

En Windows basta con hacer doble clic en `scripts\windows\abrir-interfaz.bat`.
Permite crear un presupuesto desde cero (**Nuevo**) o editar uno importado:

- **Árbol**: **arrastrar y soltar** partidas sobre un capítulo (van al final)
  o sobre otra partida (se colocan delante), en el mismo nivel o en otro;
  conservan cantidad y medición. «+ Capítulo»; clic derecho en un capítulo para crear una partida
  o un subcapítulo; clic derecho en cualquier línea para subirla, bajarla o
  quitarla (lo que quede sin usar se borra).
- **Partida**: código, unidad, resumen y texto; descompuesto con «+ Añadir
  línea…» (un concepto existente, con buscador, o uno nuevo: subpartida,
  mano de obra, material, maquinaria, subcontrata, otros o porcentaje) y
  botones ^ v x por línea; hoja de medición (uds × largo × ancho × alto o
  fórmula).
- **Subpartidas a cualquier nivel**, como los auxiliares de Presto: se pulsa
  el código de una subpartida para entrar en ella, «<< Volver» para subir, y
  arriba se ve la ruta (OBRA > C01 > P0001 > AUX1).
- Recálculo inmediato del PEM, recursos, ofertas de subcontrata y resumen.

El motor impide referencias circulares, recursos colgados de capítulos,
códigos repetidos o con caracteres reservados de BC3 (`| \ ~ #`).

## Hoja de ruta

| Hito | Contenido |
|---|---|
| **M0** Preparación | Repositorio, ADR, reglas de cálculo, estudio BC3 |
| **M1** Motor Rust + SQLite | Descompuestos, mediciones, costes y venta, revisiones |
| **M2** BC3 y Presto 8.8 | Importador, exportador, banco de pruebas con Presto 8.8 |
| **M3** Interfaz e informes | Escritorio Tauri, motor de informes PDF/Excel, 14 informes |
| **M4** Subcontratas y costes | Paquetes de trabajo, sustitución de mano de obra, ofertas |
| **M5** MCP, eXpertis, v1.0 | Servidor MCP, cambios con aprobación, exportador eXpertis, instalador |

Seguimiento en GitHub Projects e Issues (P-001 … P-024).

## Documentación

- [Arquitectura](docs/ARCHITECTURE.md)
- [Reglas de cálculo económico](docs/reglas-calculo.md)
- [Decisiones de arquitectura (ADR)](docs/adr/)
- [Matriz de registros BC3](docs/bc3/matriz-registros.md)
- [Backlog inicial](docs/backlog.md)

## Compilar y probar

Requiere Rust ≥ 1.85.

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p ppto-cli -- demo --db prueba.sqlite
```

## Participar

Buscamos tanto **programadores** como **profesionales de la presupuestación**
(validar reglas de cálculo, aportar casos sintéticos, probar con Presto).
Lee [CONTRIBUTING.md](CONTRIBUTING.md); las tareas para empezar llevan la
etiqueta `good-first-issue`.

**Nunca subas presupuestos reales de clientes**: solo datos sintéticos o con
autorización escrita.

## Licencia

[GPL-3.0-or-later](LICENSE) © 2026 Juan Carlos Cortés Molinera y colaboradores.
Ver [ADR-0002](docs/adr/0002-licencia-y-reutilizacion-ingepresupuestos.md).

Presto es una marca de RIB Spain y eXpertis una marca de su titular; este
proyecto no está afiliado a ninguna de ellas.
