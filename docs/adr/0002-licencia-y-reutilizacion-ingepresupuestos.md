# ADR-0002: Licencia GPL-3.0-or-later y reutilización de IngePresupuestos

- Estado: **Propuesto — requiere decisión del responsable del proyecto**
- Fecha: 2026-10-09
- Issue: P-002

## Contexto

### Qué es IngePresupuestos

Revisado el repositorio público `ingelibre/ingepresupuestos` (último commit
analizado `bc5a6b8`, 28-09-2026):

| Aspecto | Hallazgo |
|---|---|
| Licencia | **GPL-3.0-or-later**, con CLA que concede al autor licencia sublicenciable sobre las contribuciones |
| Autor | Ing. Marco Sumari (Perú) |
| Tecnología | Python 3.11 + PySide6 (Qt 6, LGPL), SQLite, ~108 000 líneas Python |
| Mercado | Perú y Latinoamérica: ACU, metrados, fórmula polinómica con índices INEI, norma peruana de acero |
| BC3 / FIEBDC | **No existe soporte** (sin coincidencias en el código) |
| Importadores | S10 (`.S2K`), PowerCost (`.prs`), Delphin, Excel, IFC |
| Informes | 13, en PDF, Excel, Word, ODS y ODT, con formato configurable |
| Base de datos | 39 tablas: presupuesto, catálogo, ACU, cronograma, control de obra, valorizaciones |
| Cálculo | `float` de Python con funciones de redondeo comercial *half-up* (`_r2`, `_rn`) y «parcial WYSIWYG» |
| Pruebas | 30 módulos de prueba, incluido uno de reglas críticas de negocio |

### Restricciones legales

1. Traducir su código Python a Rust produce una **obra derivada**: el
   resultado tendría que distribuirse bajo GPL-3.0 o posterior.
2. Las **ideas, reglas de negocio, flujos de trabajo y catálogos de
   funciones** no están protegidos por derechos de autor; sí lo están el
   código, los textos, las imágenes y la base de datos semilla.
3. Su CLA solo afecta a quien contribuye a *su* repositorio.

## Decisión propuesta

**Licenciar este proyecto bajo GPL-3.0-or-later** y aplicar una política de
**reutilización por ideas, no por código** mientras no se decida otra cosa:

| Se aprovecha (sin copiar código) | No se aprovecha |
|---|---|
| Reglas: redondeo half-up, parcial «lo que se ve», decimales por ámbito, detector precio ≠ descompuesto | Código Python, plantillas, iconos, `presupuestos_seed.db` |
| Idea de «precios del catálogo vs. precios del proyecto» con vista previa de cambios | Lógica peruana (ACU por cuadrilla/jornada, INEI, NTP acero) |
| Batería de pruebas de reglas críticas como **patrón** de pruebas | Importadores S10/PowerCost/Delphin (sin uso en España) |
| Formato configurable de informes (logo, márgenes, encabezados) | Asistente IA propio (se sustituye por MCP) |
| Ida y vuelta con MS Project como referencia para eXpertis | |

Si en el futuro se quiere portar código concreto, la licencia GPL-3.0-or-later
ya lo permite: se conservará el aviso de copyright de origen, se marcará el
fichero con `SPDX-License-Identifier: GPL-3.0-or-later` y se citará en
`THIRD-PARTY-NOTICES`.

### Por qué GPL y no MIT/Apache

- Mantiene abierta la posibilidad de portar código de IngePresupuestos.
- Garantiza que las mejoras de terceros (otras ingenierías, fabricantes de
  software) vuelvan a la comunidad, que es el objetivo del proyecto.
- No impide el uso comercial ni la integración con eXpertis por ficheros o
  API: comunicarse con otro programa no convierte a ese programa en obra
  derivada. *Confirmar con el implantador en P-022 si su conector exigiera
  enlazar bibliotecas propietarias.*
- Las dependencias Rust previstas (MIT/Apache-2.0/BSD/Zlib) son compatibles;
  `cargo-deny` lo verifica en cada PR.

### Contribuciones

Se usa **DCO** (`Signed-off-by` en cada commit) en lugar de CLA: menor
fricción para ingenieros externos. Contrapartida: cambiar de licencia en el
futuro requeriría el permiso de todos los contribuidores; `-or-later` cubre
el paso a versiones posteriores de la GPL.

## Alternativas consideradas

- **MIT/Apache-2.0**: máxima adopción y uso en productos cerrados; impide
  portar código GPL y permite cerrar mejoras.
- **AGPL-3.0**: añade obligación en uso por red (relevante si se ofrece el
  servidor MCP como servicio); se puede adoptar más adelante.

## Consecuencias

- `LICENSE` contiene la GPL-3.0; cada fichero lleva cabecera SPDX.
- Ningún fichero de IngePresupuestos se copia en este repositorio en v0.1.
- **Hasta que el responsable apruebe este ADR no se publica la primera
  versión** (cambiar de licencia es fácil hoy y difícil con contribuidores).
