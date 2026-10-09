# Backlog inicial

> Fuente: backlog de desarrollo aportado por el responsable del proyecto (09-10-2026). Cada tarea se publica como Issue; este fichero es la referencia versionada.

## M0 — Preparación y especificaciones

### P-001. Crear repositorio GitHub y configurar la colaboración

Etiquetas: `documentacion` · `P1-high` · Estado inicial: **Revisión técnica**

- Crear README, CONTRIBUTING, SECURITY y plantillas de Issues y Pull Requests.
- Configurar permisos, revisores y GitHub Projects.

**Criterio de aceptación:** repositorio preparado para recibir contribuciones externas.

### P-002. Evaluar IngePresupuestos

Etiquetas: `arquitectura` · `P1-high` · Estado inicial: **Revisión técnica**

- Analizar código, base de datos, cálculos y sistema de informes.
- Evaluar reutilización y compatibilidad GPL/Python/Rust.

**Criterio de aceptación:** documento ADR con decisiones técnicas justificadas.

### P-003. Definir reglas de cálculo económico

Etiquetas: `costes` · `P0-critical` · `needs-engineering-review` · Estado inicial: **Revisión técnica**

- Definir decimales, redondeos, importes, márgenes y rendimientos.
- Preparar ejemplos de cálculo verificables.

**Criterio de aceptación:** batería de pruebas económicas aprobada.

### P-004. Estudiar el estándar BC3

Etiquetas: `bc3` · `P1-high` · Estado inicial: **En desarrollo**

- Analizar FIEBDC-3/2024 y versiones históricas.
- Estudiar compatibilidad con Presto 8.8.

**Criterio de aceptación:** matriz de registros y archivos de prueba.

## M1 — Motor económico Rust y SQLite

### P-005. Crear arquitectura Rust

Etiquetas: `arquitectura` · `P0-critical` · Estado inicial: **Revisión técnica**

- Crear Cargo workspace y módulos independientes.
- Incorporar SQLite, migraciones y pruebas automáticas.

**Criterio de aceptación:** compilación y pruebas mediante GitHub Actions.

### P-006. Implementar partidas y precios descompuestos

Etiquetas: `costes` · `P0-critical` · `needs-engineering-review` · Estado inicial: **Revisión técnica**

- Crear materiales, mano de obra, maquinaria y subcontratas.
- Permitir descomposiciones jerárquicas y recursos reutilizables.

**Criterio de aceptación:** cálculos correctos y detección de referencias circulares.

### P-007. Desarrollar motor de mediciones

Etiquetas: `mediciones` · `P1-high` · `needs-engineering-review` · Estado inicial: **Revisión técnica**

- Mediciones por unidades, longitudes, superficies y volúmenes.
- Fórmulas, subtotales y agrupaciones.

**Criterio de aceptación:** cantidades reproducibles y validación de unidades.

### P-008. Implementar costes y precios de venta

Etiquetas: `costes` · `P1-high` · `needs-engineering-review` · Estado inicial: **Revisión técnica**

- Costes directos e indirectos.
- Márgenes comerciales, recargos y descuentos.
- Diferenciar margen sobre venta de recargo sobre coste.

**Criterio de aceptación:** pruebas verificadas con margen del 30 %.

### P-009. Crear revisiones de presupuestos

Etiquetas: `versiones` · `P2-medium` · Estado inicial: **Revisión técnica**

- Guardar presupuesto original, variantes y revisiones.
- Registrar modificaciones, usuarios y fechas.

**Criterio de aceptación:** modificar R2 no altera R1.

## M2 — Intercambio BC3 y Presto 8.8

### P-010. Desarrollar importador BC3

Etiquetas: `bc3` · `P0-critical` · Estado inicial: **Preparado**

- Leer conceptos, capítulos, descompuestos, mediciones y textos.
- Detectar registros incorrectos y errores de importación.

**Criterio de aceptación:** importación verificada con archivos de prueba FIEBDC.

### P-011. Desarrollar exportador BC3

Etiquetas: `bc3` · `P0-critical` · Estado inicial: **Preparado**

- Generar archivos BC3 compatibles con FIEBDC.
- Comprobar importación y exportación sucesivas.

**Criterio de aceptación:** conservar estructura, mediciones, precios e importes.

### P-012. Validar compatibilidad con Presto 8.8

Etiquetas: `bc3` · `P1-high` · `needs-engineering-review` · Estado inicial: **Backlog**

- Importar presupuestos reales autorizados.
- Exportar y abrir nuevamente desde Presto.
- Documentar las limitaciones de cada versión BC3.

**Criterio de aceptación:** pruebas económicas y de mediciones satisfactorias.

## M3 — Interfaz e informes

### P-013. Crear interfaz gráfica de escritorio

Etiquetas: `interfaz` · `P2-medium` · Estado inicial: **Backlog**

- Tauri con tablas editables.
- Árbol de capítulos, partidas y descompuestos.
- Edición de mediciones, precios y recursos.

**Criterio de aceptación:** gestionar un presupuesto sin utilizar herramientas de desarrollo.

### P-014. Crear motor de informes

Etiquetas: `informes` · `P2-medium` · Estado inicial: **Backlog**

- Plantillas configurables.
- Exportación PDF y Excel.
- Filtros, columnas, agrupaciones y subtotales.

**Criterio de aceptación:** cinco informes iniciales validados.

### P-015. Completar catálogo de informes

Etiquetas: `informes` · `P3-low` · Estado inicial: **Backlog**

- Implementar 14 informes:
- 1. Resumen de presupuesto por capítulos.
- 2. Presupuesto completo.
- 3. Presupuesto descompuesto.
- 4. Presupuesto ciego.
- 5. Mediciones detalladas.
- 6. Resumen de materiales.
- 7. Resumen de mano de obra.
- 8. Horas previstas por oficio.
- 9. Resumen de maquinaria.
- 10. Resumen de subcontratas.
- 11. Costes por capítulo.
- 12. Rentabilidad por partida.
- 13. Comparación de revisiones.
- 14. Necesidades de compra.

**Criterio de aceptación:** todos los informes exportables y sus filtros comprobados.

## M4 — Subcontratas y control de costes

### P-016. Crear paquetes de trabajo

Etiquetas: `subcontratas` · `P2-medium` · `needs-engineering-review` · Estado inicial: **Backlog**

- Agrupar actividades de distintas partidas.
- Definir los trabajos incluidos y excluidos.
- Vincular mediciones y recursos.

**Criterio de aceptación:** asociación y modificación de actividades sin alterar los descompuestos originales.

### P-017. Convertir mano de obra propia en subcontratación

Etiquetas: `subcontratas` · `P1-high` · `needs-engineering-review` · Estado inicial: **En desarrollo**

- Sustituir costes de mano de obra propia por contratos externos.
- Permitir contratación por m², ml, unidad o importe alzado.
- Mantener intactos los costes originales.
- Evitar duplicación de costes.

**Criterio de aceptación:** simulación completa de suelo radiante validada.

### P-018. Informes de mano de obra y rendimientos

Etiquetas: `mano-de-obra` · `P2-medium` · `needs-engineering-review` · Estado inicial: **Backlog**

- Horas previstas y costes por oficio.
- Rendimientos por unidad de producción.
- Comparación entre ejecución propia y subcontratada.
- Horas de personal liberadas por subcontratación.

**Criterio de aceptación:** trazabilidad entre horas originales y actualizadas.

### P-019. Gestionar ofertas de subcontratistas

Etiquetas: `compras` · `P3-low` · `needs-engineering-review` · Estado inicial: **Backlog**

- Registrar proveedores, ofertas y precios.
- Comparar ofertas con diferentes unidades de contratación.
- Registrar adjudicaciones.

**Criterio de aceptación:** seleccionar una oferta sin modificar el presupuesto comercial original.

## M5 — MCP, eXpertis y publicación

### P-020. Desarrollar servidor MCP en Rust

Etiquetas: `mcp` · `P2-medium` · Estado inicial: **Backlog**

- Consultar obras y presupuestos.
- Buscar partidas y recursos.
- Obtener mediciones y descompuestos.
- Simular modificaciones económicas.

**Criterio de aceptación:** operaciones de lectura y simulación verificadas.

### P-021. Incorporar modificaciones MCP seguras

Etiquetas: `seguridad` · `P2-medium` · Estado inicial: **Backlog**

- Solicitar aprobación antes de modificar datos.
- Registrar todas las operaciones.
- Ejecutar cambios mediante transacciones SQLite.

**Criterio de aceptación:** ninguna modificación sin autorización.

### P-022. Estudiar integración con eXpertis

Etiquetas: `expertis` · `P3-low` · Estado inicial: **Backlog**

- Identificar módulos y formatos de integración disponibles.
- Definir correspondencia de obras, capítulos, partidas y recursos.
- Analizar transferencia de costes, compras, subcontratas y mano de obra.

**Criterio de aceptación:** documento de integración validado con el implantador de eXpertis.

### P-023. Desarrollar exportador eXpertis

Etiquetas: `expertis` · `P3-low` · Estado inicial: **Backlog**

- Generar datos adaptados a las interfaces autorizadas.
- Incorporar vista previa y validaciones.
- Controlar duplicados, actualizaciones y errores.

**Criterio de aceptación:** transferencia satisfactoria en un entorno de pruebas.

### P-024. Publicar primera versión

Etiquetas: `documentacion` · `P3-low` · Estado inicial: **Backlog**

- Crear instalador Windows.
- Publicar manual técnico y de usuario.
- Preparar ejemplos de presupuestos BC3.
- Incorporar pruebas automáticas y notas de versión.

**Criterio de aceptación:** primera versión reproducible y descargable desde GitHub.

---

# Organización de GitHub Projects

## Estados del tablero

"Backlog → Preparado → En desarrollo → Revisión técnica → Pruebas → Hecho"

## Etiquetas de prioridad

- "P0-critical": bloqueante.
- "P1-high": prioridad alta.
- "P2-medium": prioridad normal.
- "P3-low": mejora no urgente.
- "good-first-issue": adecuada para nuevos colaboradores.
- "needs-engineering-review": requiere validación por un ingeniero.

## Reglas de desarrollo y colaboración

1. Cada tarea tendrá su propia Issue en GitHub.
2. Toda modificación de código se realizará mediante Pull Request.
3. Las funciones económicas deberán contar con pruebas automáticas.
4. Los cambios que afecten a mediciones, costes, márgenes y rendimientos requerirán revisión técnica de ingeniería.
5. No se utilizarán presupuestos reales de clientes en el repositorio público.
6. Se documentarán las decisiones arquitectónicas mediante ADR.
7. Se mantendrá una versión funcional y estable del programa.
8. Las versiones publicadas incluirán registro de cambios.
9. El sistema deberá admitir aportaciones de ingenieros externos.
10. Los archivos BC3 de prueba deberán utilizar datos sintéticos o autorizados.
11. Se comprobará la licencia de todas las dependencias y del código reutilizado.
12. Cada hito deberá disponer de criterios objetivos de aceptación.

## Objetivo del proyecto

Desarrollar un gestor de presupuestos de construcción e instalaciones para España, basado en Rust, SQLite y BC3, con control económico de obras, análisis de mano de obra, subcontratación, informes configurables, integración MCP y conexión con eXpertis.

El desarrollo se realizará públicamente en GitHub, favoreciendo la colaboración y revisión de otros profesionales de ingeniería.
