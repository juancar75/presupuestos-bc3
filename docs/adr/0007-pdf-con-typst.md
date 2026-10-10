# ADR-0007: Presupuesto en PDF con Typst incluido en el programa

- Estado: Propuesto
- Fecha: 2026-10-10
- Issue: P-016 (informes PDF)

## Contexto

El presupuesto que se entrega al cliente necesita calidad tipográfica de
LaTeX: texto de las partidas justificado y con **partición silábica en
castellano**, tablas que se parten bien entre páginas, portada con logo,
fecha y revisión, y una hoja final de observaciones. El equipo trabaja en
Windows y no debe tener que instalar ni mantener una distribución LaTeX.

Opciones estudiadas:

| Opción | Calidad | Instalación | Velocidad |
|---|---|---|---|
| LaTeX del sistema (MiKTeX/TeX Live) | LaTeX | 1–4 GB en cada PC | segundos |
| Tectonic (LaTeX en Rust) | LaTeX | ~300 MB de paquetes la primera vez | segundos |
| **Typst como librería** | Knuth-Plass, partición `hypher` (patrones TeX) | ninguna | milisegundos |
| Generar PDF a mano (printpdf…) | sin justificado ni partición | ninguna | milisegundos |

## Decisión

- Nuevo crate `ppto-pdf` con **Typst 0.15** (Apache-2.0) incluido mediante
  `typst-as-lib`, y fuentes de `typst-assets` (New Computer Modern, el
  aspecto de LaTeX).
- El diseño vive en una **plantilla Typst** (`crates/ppto-pdf/plantillas/presupuesto.typ`)
  que el usuario puede copiar, editar y usar sin recompilar.
- Rust prepara los datos (`datos.json`) con todas las cifras ya calculadas y
  formateadas por `ppto-core`: la plantilla no calcula nada.
- Requiere Rust ≥ 1.92 (versión mínima del proyecto actualizada).

## Consecuencias

- PDF reproducible en cualquier PC, sin dependencias externas.
- El ejecutable crece unos megabytes (Typst y fuentes).
- Quien conozca LaTeX tiene que aprender la sintaxis de Typst para retocar la
  plantilla (más sencilla; documentación en https://typst.app/docs).
