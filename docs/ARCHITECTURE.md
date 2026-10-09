# Arquitectura

> Estado: **borrador v0.1** (M0/M1). Cada decisión de peso tiene su ADR en [`docs/adr/`](adr/).

## 1. Objetivo y alcance

Gestor de presupuestos de construcción e instalaciones para el mercado español:
mediciones, precios descompuestos, control de costes (materiales, mano de obra,
maquinaria, subcontratas), intercambio **BC3/FIEBDC-3** con validación frente a
**Presto 8.8**, informes configurables en PDF y Excel, servidor **MCP** y
exportación futura a **eXpertis**.

Requisitos no funcionales que condicionan todo el diseño:

| Requisito | Consecuencia |
|---|---|
| Exactitud económica | Aritmética decimal exacta (`rust_decimal`), `f64` prohibido por lint, redondeo comercial explícito ([ADR-0003](adr/0003-aritmetica-decimal-y-redondeo.md)) |
| Trazabilidad | Revisiones inmutables, auditoría de cada escritura, escenarios que no alteran el original |
| Interoperabilidad | Modelo interno alineado con los registros FIEBDC-3 para ida y vuelta sin pérdidas |
| Uso en oficina técnica | Escritorio Windows primero, un solo fichero `.sqlite` por obra, sin servidor |
| Colaboración abierta | Núcleo sin E/S, probado de forma aislada; módulos con fronteras claras |

## 2. Vista de módulos

```
                 ┌────────────────────────┐     ┌──────────────────────┐
  Ingeniero ───▶ │ apps/escritorio (Tauri)│     │  Claude / agentes    │
                 └──────────┬─────────────┘     └──────────┬───────────┘
                            │ órdenes                      │ MCP (stdio / HTTP)
                 ┌──────────▼─────────────────────────────▼───────────┐
                 │            ppto-app  (casos de uso)                 │
                 │  abrir obra · editar · simular · aprobar cambios    │
                 └───┬──────────┬──────────┬──────────┬────────────┬───┘
                     │          │          │          │            │
              ┌──────▼───┐ ┌────▼────┐ ┌───▼─────┐ ┌──▼───────┐ ┌──▼────────┐
              │ ppto-bc3 │ │ppto-db  │ │ppto-    │ │ppto-     │ │ppto-      │
              │ import/  │ │SQLite,  │ │informes │ │expertis  │ │mcp        │
              │ export   │ │revis.,  │ │PDF/XLSX │ │(M5)      │ │(M5)       │
              └──────┬───┘ │auditoría│ └───┬─────┘ └──┬───────┘ └──┬────────┘
                     │     └────┬────┘     │          │            │
                     └──────────┴────┬─────┴──────────┴────────────┘
                              ┌──────▼──────┐
                              │  ppto-core  │  modelo + motor económico
                              │  (sin E/S)  │  (puro, determinista)
                              └─────────────┘
```

| Crate | Estado | Responsabilidad |
|---|---|---|
| `ppto-core` | **prototipo** | Conceptos, descomposición, mediciones, precios, explosión de recursos, venta, escenarios de subcontrata |
| `ppto-db` | **prototipo** | SQLite, migraciones, revisiones, bloqueo, auditoría |
| `ppto-cli` | **prototipo** | Demostración y verificación reproducible (`ppto demo`) |
| `ppto-bc3` | M2 | Lector/escritor FIEBDC-3, cp1252/UTF-8, informe de incidencias |
| `ppto-informes` | M3 | Plantillas, filtros, agrupaciones; PDF y XLSX |
| `apps/escritorio` | M3 | Tauri 2: árbol, tablas editables, mediciones |
| `ppto-mcp` | M5 | Herramientas de lectura y simulación; escrituras con aprobación |
| `ppto-expertis` | M5 | Exportación a las interfaces que autorice el implantador |

Regla de dependencias: **todo depende de `ppto-core`; `ppto-core` no depende de
nada con E/S**. Así el motor se prueba en milisegundos y cualquier
colaborador puede verificar un cálculo sin base de datos ni interfaz.

## 3. Modelo de datos

El modelo replica la estructura de FIEBDC-3 para que BC3 sea un formato de
intercambio natural y no una traducción con pérdidas.

| Entidad | Equivalente BC3 | Notas |
|---|---|---|
| `Concepto` (código, unidad, resumen, naturaleza, precio, texto) | `~C`, `~T` | Capítulos, partidas, auxiliares y recursos básicos son conceptos |
| `LineaDescomposicion` (hijo, factor, rendimiento) | `~D` | En capítulos, el rendimiento es la medición total |
| `Medicion` / `LineaMedicion` (tipo, comentario, uds, long, anch, alt, fórmula) | `~M` | Clave (padre, hijo) |
| `Decimales` | `~K` | Correspondencia exacta pendiente de P-004 |
| `PaqueteTrabajo`, `Asignacion`, `Contratacion` | — (propio) | No viaja en BC3; se exporta a eXpertis |
| Revisión | — (propio) | Copia completa por revisión ([ADR-0004](adr/0004-revisiones-como-copias.md)) |

Naturalezas: `Capitulo`, `Partida`, `ManoObra`, `Maquinaria`, `Material`,
`Subcontrata`, `Porcentaje`, `Otros`.

### Esquema SQLite (v1)

`presupuestos` 1─N `revisiones` 1─N {`conceptos`, `descomposicion`,
`medicion_lineas`} y `auditoria`. Importes y cantidades se guardan como
`TEXT` decimal exacto. Ver
[`crates/ppto-db/migrations/0001_esquema_inicial.sql`](../crates/ppto-db/migrations/0001_esquema_inicial.sql).

## 4. Motor económico

Reglas completas en [`reglas-calculo.md`](reglas-calculo.md). Resumen:

1. **Lo que se ve es lo que se multiplica**: cada operando se redondea a sus
   decimales de presentación antes de multiplicar.
2. Precio de partida = Σ importes de línea redondeados.
3. Importe en capítulo = medición redondeada × precio de partida.
4. La explosión de recursos multiplica cantidades totales; su diferencia con
   el PEM se informa como **descuadre de redondeo**.
5. Venta: se distingue **margen sobre venta** de **recargo sobre coste**.
6. Subcontratación: escenario calculado sobre el presupuesto inmutable, con
   prohibición de doble sustitución ([ADR-0005](adr/0005-subcontratacion-como-escenario.md)).

## 5. Intercambio BC3 y Presto 8.8 (M2)

- Lectura tolerante, escritura estricta: el importador acepta variantes y
  registra cada incidencia con línea y registro; el exportador genera
  FIEBDC-3 conforme y en la codificación declarada en `~V`.
- Pruebas de **ida y vuelta**: BC3 → modelo → BC3 → modelo debe conservar
  estructura, mediciones, precios e importes al céntimo.
- Validación con Presto 8.8 en un banco de pruebas documentado
  ([`docs/bc3/matriz-registros.md`](bc3/matriz-registros.md)): solo ficheros
  sintéticos o con autorización expresa.

## 6. MCP (M5)

Mismo binario para Claude Desktop (stdio) y clientes remotos (HTTP).
Herramientas de **lectura** (obras, partidas, recursos, mediciones,
descompuestos) y de **simulación** (cambios de precio, subcontratación,
márgenes) sin efectos. Las **escrituras** se proponen como un cambio
pendiente que el usuario aprueba en la aplicación y se ejecutan en una
transacción SQLite registrada en `auditoria` (P-021).

## 7. Informes (M3)

Motor de plantillas declarativas (columnas, filtros, agrupación, subtotales)
sobre consultas del núcleo; dos salidas: PDF y XLSX. Catálogo de 14 informes
en P-015. Los informes de recursos parten de la explosión del motor, de modo
que horas por oficio, compras y subcontratas cuadran con el presupuesto.

## 8. Calidad y verificación

- Cada función económica con pruebas automáticas y valores calculados a mano
  en [`docs/casos-validacion/`](casos-validacion/).
- CI en Linux y Windows: `fmt`, `clippy -D warnings`, pruebas, licencias de
  dependencias (`cargo-deny`).
- Cambios en mediciones, costes, márgenes o rendimientos: etiqueta
  `needs-engineering-review` y aprobación de un revisor técnico.
- Versionado semántico; cada versión con entrada en `CHANGELOG.md`.

## 9. Riesgos abiertos

| Riesgo | Mitigación |
|---|---|
| Diferencias de redondeo con Presto 8.8 | Banco de pruebas P-012 y decimales configurables por presupuesto |
| Ambigüedades de FIEBDC-3 (porcentajes, fórmulas, `~K`) | Matriz de registros P-004; comportamiento configurable donde haya divergencias |
| Interfaces de eXpertis no públicas | P-022 con el implantador antes de escribir código |
| Rendimiento con presupuestos grandes | Caché de precios por cálculo; pruebas de carga en M2 con BC3 sintéticos de 10⁴ conceptos |
