// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-legalizacion
//!
//! Legalización de una disciplina de la obra (RITE o PCI, con RSIF) como
//! **una partida descompuesta** en el capítulo `LEG`, en la que figuran todos
//! los costes:
//!
//! 1. **Mano de obra técnica** por perfil (senior, junior, técnico,
//!    documentación) a su coste horario, para la tramitación (as-built,
//!    cálculos, libro de uso y mantenimiento, documentación 3.1.5, CIE del
//!    eléctrico, dirección de obra y gestión del expediente), la redacción del
//!    proyecto propio y las justificaciones RITE y RSIF. Las horas salen del
//!    honorario de baremo (% del PEM por tramos con mínimos), descontado el
//!    margen.
//! 2. **Visitas de la OCA**: horas del técnico en la inspección.
//! 3. **Desplazamientos**: horas de viaje, kilometraje, aparcamiento y dietas
//!    de cada visita (dirección de obra y OCA).
//! 4. **Costes indirectos, gestión y beneficio**: % sobre lo anterior.
//! 5. **Repercutidos** sin margen: tasas de la Comunidad, EICI y visados.
//!
//! El baremo (honorarios, costes horarios y tablas) es del usuario y vive
//! fuera del repositorio, en `legalizacion.json` de la carpeta de
//! configuración, con una versión por año. [`Baremo::ejemplo`] lleva valores
//! **sintéticos** para pruebas y como plantilla.

use ppto_core::decimales::redondear;
use ppto_core::formato::{eur, num};
use ppto_core::{Concepto, ErrorMotor, LineaDescomposicion, Naturaleza, Presupuesto};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Fichero del baremo en la carpeta de configuración.
pub const FICHERO: &str = "legalizacion.json";
/// Capítulo que agrupa la legalización en el presupuesto.
pub const CAPITULO: &str = "LEG";

const CIEN: Decimal = Decimal::ONE_HUNDRED;

fn d(n: u32) -> Decimal {
    Decimal::from(n)
}

/// Ruta del baremo (`%APPDATA%\presupuestos-bc3\legalizacion.json`).
pub fn ruta() -> Option<PathBuf> {
    Some(ppto_propuestas::carpeta_config()?.join(FICHERO))
}

// ------------------------------------------------------------------ baremo

/// Tramo de una tabla: hasta `hasta` (incluido; `None` = sin límite) vale `valor`.
///
/// En las tablas acumulativas (tramitación, visado) `valor` es el porcentaje
/// o coeficiente que se aplica a la parte de la base dentro del tramo; en las
/// de escalón (tasas, EICI) es el importe fijo del tramo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tramo {
    pub hasta: Option<Decimal>,
    pub valor: Decimal,
}

fn t(hasta: Option<Decimal>, valor: Decimal) -> Tramo {
    Tramo { hasta, valor }
}

/// Parte de la base en cada tramo acumulativo: `(desde, hasta, valor, base_en_tramo)`.
fn por_tramos(tramos: &[Tramo], base: Decimal) -> Vec<(Decimal, Option<Decimal>, Decimal, Decimal)> {
    let mut r = Vec::new();
    let mut desde = Decimal::ZERO;
    for tr in tramos {
        if base <= desde {
            break;
        }
        let tope = tr.hasta.map_or(base, |h| h.min(base));
        let parte = (tope - desde).max(Decimal::ZERO);
        if parte > Decimal::ZERO {
            r.push((desde, tr.hasta, tr.valor, parte));
        }
        match tr.hasta {
            Some(h) => desde = h,
            None => break,
        }
    }
    r
}

/// Valor del primer tramo de escalón que contiene `x`; `None` si lo supera.
fn escalon(tramos: &[Tramo], x: Decimal) -> Option<Decimal> {
    tramos
        .iter()
        .find(|tr| tr.hasta.is_none_or(|h| x <= h))
        .map(|tr| tr.valor)
}

/// Reparto de las horas de una tarea entre perfiles (en %; suman 100).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reparto {
    pub senior: Decimal,
    pub junior: Decimal,
    pub tecnico: Decimal,
    pub documentacion: Decimal,
}

impl Default for Reparto {
    fn default() -> Self {
        Self {
            senior: d(25),
            junior: d(75),
            tecnico: Decimal::ZERO,
            documentacion: Decimal::ZERO,
        }
    }
}

fn reparto_tramitacion() -> Reparto {
    Reparto {
        senior: d(15),
        junior: d(35),
        tecnico: d(30),
        documentacion: d(20),
    }
}

fn reparto_memoria() -> Reparto {
    Reparto {
        senior: d(10),
        junior: d(50),
        tecnico: d(10),
        documentacion: d(30),
    }
}

fn reparto_redaccion() -> Reparto {
    Reparto {
        senior: d(40),
        junior: d(45),
        tecnico: Decimal::ZERO,
        documentacion: d(15),
    }
}

/// Honorarios, costes y tablas de un año. Todos los importes sin IVA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Baremo {
    pub anio: i32,
    /// Procedencia de los datos (p. ej. «Tablas PRMEM 2026, honorarios AIDESEGI»).
    pub nota: String,

    // --- honorarios de baremo (precio de venta, margen incluido)
    /// Proyectos: tramitación y dirección de la instalación, % del PEM de la
    /// disciplina por tramos acumulativos.
    pub tramitacion: Vec<Tramo>,
    /// Memorias técnicas: elaboración y tramitación, % del PEM por tramos
    /// acumulativos (vacío = los de proyectos).
    pub tramitacion_memoria: Vec<Tramo>,
    /// Mínimo por expediente en proyectos (y en memorias si el de memoria es cero).
    pub tramitacion_minimo: Decimal,
    /// Mínimo por expediente en memorias técnicas (0 = el de proyectos).
    pub tramitacion_minimo_memoria: Decimal,
    /// Mínimo por vivienda con uso Viviendas (se cobra el mayor).
    pub minimo_vivienda: Decimal,
    /// Recargo (%) cuando el proyecto es de otro técnico.
    pub recargo_ajeno: Decimal,
    /// Redacción del proyecto propio: % del PEM por tramos acumulativos.
    pub redaccion: Vec<Tramo>,
    pub redaccion_minimo: Decimal,
    /// Justificación RITE aparte: % del PEM RITE con mínimo.
    pub justif_rite_pct: Decimal,
    pub justif_rite_minimo: Decimal,
    /// Justificación RSIF: por circuito de cada modelo, por circuito repetido
    /// (0 = como uno distinto) y mínimo.
    pub rsif_circuito: Decimal,
    pub rsif_repetido: Decimal,
    pub rsif_minimo: Decimal,

    // --- costes
    /// Coste horario de cada perfil (€/h).
    pub coste_senior: Decimal,
    pub coste_junior: Decimal,
    pub coste_tecnico: Decimal,
    pub coste_documentacion: Decimal,
    /// Costes indirectos, gestión y beneficio (%) sobre mano de obra y desplazamientos.
    pub margen: Decimal,
    /// Reparto de horas de la tramitación de proyectos y de memorias.
    pub reparto_tramitacion: Reparto,
    pub reparto_memoria: Reparto,
    pub reparto_redaccion: Reparto,
    /// Visitas a obra que se proponen por defecto en memorias y en proyectos.
    pub visitas_memoria: u32,
    pub visitas_proyecto: u32,
    pub reparto_justificacion: Reparto,
    /// Horas del técnico en cada visita de la OCA.
    pub oca_horas: Decimal,
    /// Viaje habitual (sin datos de la obra): km de ida y vuelta y horas.
    pub viaje_km: Decimal,
    pub viaje_horas: Decimal,
    /// €/km, aparcamiento por viaje y dieta por viaje fuera de zona.
    pub km: Decimal,
    pub aparcamiento: Decimal,
    pub dieta: Decimal,
    /// Km de ida y vuelta a partir de los cuales se paga dieta.
    pub km_umbral: Decimal,

    // --- repercutidos
    /// Derechos de visado: coeficiente por tramos acumulativos sobre el PEM.
    pub visado: Vec<Tramo>,
    /// Visado del certificado de dirección de obra (mismo importe que el proyecto).
    pub visado_direccion_obra: bool,
    /// Tasa RITE de proyecto por valor de la inversión (escalón).
    pub tasa_rite_proyecto: Vec<Tramo>,
    pub tasa_rite_memoria_vivienda: Decimal,
    pub tasa_rite_memoria_otro: Decimal,
    pub eici_rite_memoria: Decimal,
    pub eici_rite_memoria_primario: Decimal,
    /// EICI de proyecto RITE por potencia en kW (escalón); por encima, `+ por_100kw`.
    pub eici_rite_kw: Vec<Tramo>,
    pub eici_rite_por_100kw: Decimal,
    /// EICI de proyecto para edificios de N viviendas sin sala de máquinas
    /// (escalón); por encima, `base + por_vivienda × N`.
    pub eici_rite_viviendas: Vec<Tramo>,
    pub eici_rite_viviendas_base: Decimal,
    pub eici_rite_por_vivienda: Decimal,
    pub eici_rite_primario: Decimal,
    /// EICI de segundas visitas y posteriores.
    pub eici_segunda_visita: Decimal,
    /// Tasa PCI de proyecto por inversión (escalón); por encima, `bloque × nº de bloques`.
    pub tasa_pci: Vec<Tramo>,
    pub tasa_pci_bloque: Decimal,
    pub tasa_pci_tam_bloque: Decimal,
    pub tasa_pci_memoria: Decimal,
    /// EICI PCI de memoria por nº de instalaciones (1, 2, … el último vale para más).
    pub eici_pci_memoria: Vec<Decimal>,
    /// EICI PCI de proyecto: filas por nº de instalaciones, columnas por superficie.
    pub eici_pci_proyecto: Vec<Vec<Decimal>>,
    /// Límites de superficie (m²) de las columnas: columna i si S < límite i.
    pub eici_pci_superficies: Vec<Decimal>,
    pub eici_pci_presion: Decimal,
    /// Registro de instalación frigorífica (RSIF): EICI por nivel y tasa.
    pub eici_if_memoria_a2l: Decimal,
    pub eici_if_memoria_l1: Decimal,
    pub eici_if_proyecto: Decimal,
    pub eici_if_proyecto_sala: Decimal,
    pub tasa_if: Decimal,
}

impl Default for Baremo {
    fn default() -> Self {
        let z = Decimal::ZERO;
        Self {
            anio: 0,
            nota: String::new(),
            tramitacion: Vec::new(),
            tramitacion_minimo: z,
            tramitacion_minimo_memoria: z,
            minimo_vivienda: z,
            recargo_ajeno: z,
            redaccion: Vec::new(),
            redaccion_minimo: z,
            justif_rite_pct: z,
            justif_rite_minimo: z,
            rsif_circuito: z,
            rsif_repetido: z,
            rsif_minimo: z,
            coste_senior: z,
            coste_junior: z,
            coste_tecnico: z,
            coste_documentacion: z,
            margen: z,
            reparto_tramitacion: reparto_tramitacion(),
            reparto_memoria: reparto_memoria(),
            visitas_memoria: 1,
            visitas_proyecto: 2,
            tramitacion_memoria: Vec::new(),
            reparto_redaccion: reparto_redaccion(),
            reparto_justificacion: Reparto::default(),
            oca_horas: z,
            viaje_km: z,
            viaje_horas: z,
            km: z,
            aparcamiento: z,
            dieta: z,
            km_umbral: z,
            visado: Vec::new(),
            visado_direccion_obra: true,
            tasa_rite_proyecto: Vec::new(),
            tasa_rite_memoria_vivienda: z,
            tasa_rite_memoria_otro: z,
            eici_rite_memoria: z,
            eici_rite_memoria_primario: z,
            eici_rite_kw: Vec::new(),
            eici_rite_por_100kw: z,
            eici_rite_viviendas: Vec::new(),
            eici_rite_viviendas_base: z,
            eici_rite_por_vivienda: z,
            eici_rite_primario: z,
            eici_segunda_visita: z,
            tasa_pci: Vec::new(),
            tasa_pci_bloque: z,
            tasa_pci_tam_bloque: z,
            tasa_pci_memoria: z,
            eici_pci_memoria: Vec::new(),
            eici_pci_proyecto: Vec::new(),
            eici_pci_superficies: Vec::new(),
            eici_pci_presion: z,
            eici_if_memoria_a2l: z,
            eici_if_memoria_l1: z,
            eici_if_proyecto: z,
            eici_if_proyecto_sala: z,
            tasa_if: z,
        }
    }
}

impl Baremo {
    /// Baremo con valores **sintéticos y redondos** (no son tarifas reales):
    /// sirve para las pruebas y como plantilla que el usuario sustituye.
    pub fn ejemplo(anio: i32) -> Self {
        let m = |n: u32| Some(d(n));
        Self {
            anio,
            nota: "VALORES DE EJEMPLO: sustitúyalos por sus honorarios, costes y las tablas oficiales del año".into(),
            tramitacion: vec![t(m(100_000), d(1)), t(None, Decimal::new(5, 1))],
            tramitacion_minimo: d(500),
            tramitacion_minimo_memoria: d(300),
            tramitacion_memoria: vec![t(m(50_000), Decimal::new(15, 1)), t(None, Decimal::new(75, 2))],
            minimo_vivienda: d(50),
            recargo_ajeno: d(20),
            redaccion: vec![t(m(100_000), Decimal::new(15, 1)), t(None, d(1))],
            redaccion_minimo: d(600),
            justif_rite_pct: Decimal::new(2, 1),
            justif_rite_minimo: d(200),
            rsif_circuito: d(100),
            rsif_repetido: d(20),
            rsif_minimo: d(100),
            coste_senior: d(50),
            coste_junior: d(40),
            coste_tecnico: d(40),
            coste_documentacion: d(30),
            margen: d(25),
            oca_horas: d(2),
            viaje_km: d(40),
            viaje_horas: d(1),
            km: Decimal::new(25, 2),
            aparcamiento: d(10),
            dieta: d(30),
            km_umbral: d(100),
            visado: vec![t(m(100_000), Decimal::new(2, 3)), t(None, Decimal::new(1, 3))],
            tasa_rite_proyecto: vec![t(m(10_000), d(10)), t(None, d(50))],
            tasa_rite_memoria_vivienda: d(3),
            tasa_rite_memoria_otro: d(6),
            eici_rite_memoria: d(60),
            eici_rite_memoria_primario: d(120),
            eici_rite_kw: vec![t(m(100), d(150)), t(m(1000), d(800))],
            eici_rite_por_100kw: d(10),
            eici_rite_viviendas: vec![t(m(10), d(150)), t(m(50), d(400))],
            eici_rite_viviendas_base: d(300),
            eici_rite_por_vivienda: d(4),
            eici_rite_primario: d(150),
            eici_segunda_visita: d(300),
            tasa_pci: vec![t(m(10_000), d(50)), t(m(100_000), d(200))],
            tasa_pci_bloque: d(10),
            tasa_pci_tam_bloque: d(5000),
            eici_pci_memoria: vec![d(200), d(250), d(300)],
            eici_pci_proyecto: vec![vec![d(400), d(500)], vec![d(450), d(550)]],
            eici_pci_superficies: vec![d(1000)],
            eici_pci_presion: d(150),
            eici_if_memoria_a2l: d(120),
            eici_if_memoria_l1: d(130),
            eici_if_proyecto: d(250),
            eici_if_proyecto_sala: d(350),
            tasa_if: d(35),
            ..Self::default()
        }
    }

    fn coste(&self, p: Perfil) -> Decimal {
        match p {
            Perfil::Senior => self.coste_senior,
            Perfil::Junior => self.coste_junior,
            Perfil::Tecnico => self.coste_tecnico,
            Perfil::Documentacion => self.coste_documentacion,
        }
    }
}

/// Baremos de varios años en un fichero.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Baremos {
    pub versiones: Vec<Baremo>,
}

impl Baremos {
    pub fn cargar(ruta: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(ruta) {
            Ok(t) if t.trim().is_empty() => Ok(Self::default()),
            Ok(t) => serde_json::from_str(&t).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} dañado: {e}", ruta.display()),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Guarda con el fichero bloqueado (lo comparten varias ventanas y equipos).
    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        let texto = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        ppto_propuestas::con_bloqueo(ruta, || std::fs::write(ruta, texto))
    }

    /// Baremo del año `anio` o, si no hay, el último anterior; si todos son
    /// posteriores, el más antiguo.
    pub fn vigente(&self, anio: i32) -> Option<&Baremo> {
        self.versiones
            .iter()
            .filter(|b| b.anio <= anio)
            .max_by_key(|b| b.anio)
            .or_else(|| self.versiones.iter().min_by_key(|b| b.anio))
    }

    /// Crea (copiando el último) o devuelve el baremo de `anio`.
    pub fn del_anio(&mut self, anio: i32) -> &mut Baremo {
        if let Some(i) = self.versiones.iter().position(|b| b.anio == anio) {
            return &mut self.versiones[i];
        }
        let mut nuevo = self.vigente(anio).cloned().unwrap_or_else(|| Baremo::ejemplo(anio));
        nuevo.anio = anio;
        self.versiones.push(nuevo);
        self.versiones.sort_by_key(|b| b.anio);
        let i = self.versiones.iter().position(|b| b.anio == anio).unwrap_or(0);
        &mut self.versiones[i]
    }
}

// --------------------------------------------------------------- expediente

// --------------------------------------------------------------- expediente

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disciplina {
    Rite,
    Pci,
}

impl Disciplina {
    pub fn sigla(self) -> &'static str {
        match self {
            Self::Rite => "RITE",
            Self::Pci => "PCI",
        }
    }
    fn nombre(self) -> &'static str {
        match self {
            Self::Rite => "instalaciones térmicas (RITE)",
            Self::Pci => "protección contra incendios (RIPCI)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Documento {
    Memoria,
    Proyecto,
}

/// Uso del edificio: cambia la tasa y la EICI de las memorias, el mínimo por
/// vivienda y la tabla EICI de bloques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Uso {
    Terciario,
    Viviendas,
}

/// Registro de instalación frigorífica que acompaña a la justificación RSIF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistroIf {
    MemoriaA2l,
    MemoriaL1,
    Proyecto,
    ProyectoConSala,
}

/// Datos de un expediente (una disciplina de la obra).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expediente {
    pub disciplina: Disciplina,
    pub documento: Documento,
    /// PEM de la disciplina (base del % de los honorarios y de la tasa).
    pub pem: Decimal,
    /// Proyecto redactado por otro técnico.
    pub ajeno: bool,
    pub uso: Uso,
    /// Nº de viviendas (solo con uso Viviendas; 1 = unifamiliar).
    pub viviendas: u32,
    /// RITE: potencia térmica (kW) para la EICI de proyecto.
    pub potencia_kw: Decimal,
    /// RITE: circuito primario frigorífico.
    pub primario_frio: bool,
    /// PCI: nº de instalaciones (extintores, BIE, detección…).
    pub instalaciones_pci: u32,
    /// PCI: superficie (m²) para la EICI de proyecto.
    pub superficie: Decimal,
    pub presion_diferencial: bool,
    /// Justificación RITE (memorias RITE y proyectos de otro técnico; en los
    /// proyectos propios va en la redacción).
    pub justificar_rite: bool,
    /// Circuitos frigoríficos a justificar por RSIF (0 = sin RSIF) y, de
    /// ellos, modelos distintos (0 = todos distintos).
    pub circuitos_rsif: u32,
    pub modelos_rsif: u32,
    pub registro_if: Option<RegistroIf>,
    /// Visitas de dirección de obra y segundas visitas de la OCA.
    pub visitas_obra: u32,
    pub segundas_visitas: u32,
    /// Km de ida y vuelta y horas por viaje (0 = viaje habitual del baremo).
    pub km: Decimal,
    pub horas_viaje: Decimal,
    /// Repercutir tasas, EICI y visado (si no, los paga la propiedad).
    pub repercutir: bool,
}

impl Expediente {
    /// Viviendas que cuentan: 0 en terciario, al menos 1 en viviendas.
    pub fn n_viviendas(&self) -> u32 {
        match self.uso {
            Uso::Terciario => 0,
            Uso::Viviendas => self.viviendas.max(1),
        }
    }

    pub fn nuevo(disciplina: Disciplina, documento: Documento, pem: Decimal) -> Self {
        Self {
            disciplina,
            documento,
            pem,
            ajeno: false,
            uso: Uso::Terciario,
            viviendas: 1,
            potencia_kw: Decimal::ZERO,
            primario_frio: false,
            instalaciones_pci: 1,
            superficie: Decimal::ZERO,
            presion_diferencial: false,
            justificar_rite: disciplina == Disciplina::Rite && documento == Documento::Memoria,
            circuitos_rsif: u32::from(disciplina == Disciplina::Rite),
            modelos_rsif: 0,
            registro_if: None,
            visitas_obra: if documento == Documento::Proyecto { 2 } else { 1 },
            segundas_visitas: 0,
            km: Decimal::ZERO,
            horas_viaje: Decimal::ZERO,
            repercutir: true,
        }
    }

    /// Justificación RITE aparte por defecto: en memorias RITE y en proyectos
    /// de otro técnico; en los proyectos propios va en la redacción.
    pub fn justificar_rite_por_defecto(&self) -> bool {
        self.disciplina == Disciplina::Rite && (!self.proyecto() || self.ajeno)
    }

    fn proyecto(&self) -> bool {
        self.documento == Documento::Proyecto
    }
}

// ----------------------------------------------------------------- cálculo

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perfil {
    Senior,
    Junior,
    Tecnico,
    Documentacion,
}

impl Perfil {
    const TODOS: [Perfil; 4] = [Perfil::Senior, Perfil::Junior, Perfil::Tecnico, Perfil::Documentacion];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::Senior => "Ingeniero senior",
            Self::Junior => "Ingeniero junior",
            Self::Tecnico => "Técnico",
            Self::Documentacion => "Técnico de documentación",
        }
    }

    fn parte(self, r: &Reparto) -> Decimal {
        match self {
            Self::Senior => r.senior,
            Self::Junior => r.junior,
            Self::Tecnico => r.tecnico,
            Self::Documentacion => r.documentacion,
        }
    }
}

/// Clase de línea: decide si lleva margen y cómo se suma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    ManoObra,
    Indirecto,
    Margen,
    Repercutido,
}

/// Línea del descompuesto de la partida de legalización.
#[derive(Debug, Clone, PartialEq)]
pub struct Linea {
    pub resumen: String,
    pub unidad: String,
    pub cantidad: Decimal,
    pub precio: Decimal,
    pub tipo: Tipo,
}

impl Linea {
    pub fn importe(&self) -> Decimal {
        redondear(self.cantidad * self.precio, 2)
    }

    pub fn naturaleza(&self) -> Naturaleza {
        match self.tipo {
            Tipo::ManoObra => Naturaleza::ManoObra,
            Tipo::Margen => Naturaleza::Porcentaje,
            Tipo::Indirecto | Tipo::Repercutido => Naturaleza::Otros,
        }
    }
}

fn importe_fijo(resumen: String, precio: Decimal, tipo: Tipo) -> Linea {
    Linea {
        resumen,
        unidad: "ud".into(),
        cantidad: Decimal::ONE,
        precio: redondear(precio, 2),
        tipo,
    }
}

fn por_unidad(resumen: String, unidad: &str, n: Decimal, precio: Decimal, tipo: Tipo) -> Linea {
    Linea {
        resumen,
        unidad: unidad.into(),
        cantidad: redondear(n, 3),
        precio: redondear(precio, 2),
        tipo,
    }
}

/// Grupo de líneas con su explicación (para la vista previa).
#[derive(Debug, Clone, PartialEq)]
pub struct Bloque {
    pub titulo: String,
    /// Cómo se ha calculado (honorario de baremo, mínimo aplicado…).
    pub regla: String,
    pub lineas: Vec<Linea>,
}

impl Bloque {
    pub fn importe(&self) -> Decimal {
        self.lineas.iter().map(Linea::importe).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calculo {
    pub resumen: String,
    pub texto: String,
    pub bloques: Vec<Bloque>,
    /// Mano de obra y desplazamientos (coste, sin margen).
    pub coste: Decimal,
    pub margen: Decimal,
    pub repercutidos: Decimal,
    /// Horas de mano de obra (incluidas las de viaje).
    pub horas: Decimal,
    pub avisos: Vec<String>,
}

impl Calculo {
    /// Honorarios: coste + margen.
    pub fn honorarios(&self) -> Decimal {
        self.coste + self.margen
    }

    pub fn total(&self) -> Decimal {
        self.honorarios() + self.repercutidos
    }

    pub fn lineas(&self) -> impl Iterator<Item = &Linea> {
        self.bloques.iter().flat_map(|b| &b.lineas)
    }
}

/// Número sin ceros sobrantes («25», «0,7», «0,26»).
fn corto(v: Decimal) -> String {
    let n = v.normalize();
    num(n, n.scale())
}

/// Honorario por tramos acumulativos y su explicación.
fn por_pem(tramos: &[Tramo], pem: Decimal) -> (Decimal, String) {
    let partes = por_tramos(tramos, pem);
    let importe = partes
        .iter()
        .map(|(_, _, pct, parte)| redondear(*parte * *pct / CIEN, 2))
        .sum();
    let txt = partes
        .iter()
        .map(|(_, _, pct, parte)| format!("{} % de {}", corto(*pct), eur(*parte)))
        .collect::<Vec<_>>()
        .join(" + ");
    (importe, txt)
}

/// Convierte un honorario de baremo (con margen) en horas por perfil a coste.
fn horas_de(b: &Baremo, que: &str, honorario: Decimal, r: &Reparto) -> Vec<Linea> {
    let tarifa: Decimal = Perfil::TODOS.iter().map(|p| p.parte(r) * b.coste(*p)).sum::<Decimal>() / CIEN;
    if tarifa <= Decimal::ZERO {
        return vec![importe_fijo(
            format!("{que} (sin costes horarios en el baremo)"),
            honorario,
            Tipo::ManoObra,
        )];
    }
    let coste = honorario * CIEN / (CIEN + b.margen);
    let horas = coste / tarifa;
    Perfil::TODOS
        .iter()
        .filter(|p| !p.parte(r).is_zero())
        .map(|p| {
            por_unidad(
                format!("{}: {que}", p.nombre()),
                "h",
                horas * p.parte(r) / CIEN,
                b.coste(*p),
                Tipo::ManoObra,
            )
        })
        .collect()
}

/// Calcula la partida de legalización de un expediente con un baremo.
pub fn calcular(b: &Baremo, e: &Expediente) -> Calculo {
    let mut c = Calculo::default();
    let proyecto = e.proyecto();
    let doc = if proyecto { "proyecto" } else { "memoria técnica" };
    let viviendas = e.n_viviendas();
    let n_viv = d(viviendas);

    // 1. Tramitación
    let minimo = if !proyecto && !b.tramitacion_minimo_memoria.is_zero() {
        b.tramitacion_minimo_memoria
    } else {
        b.tramitacion_minimo
    };
    let tramos = if !proyecto && !b.tramitacion_memoria.is_empty() {
        &b.tramitacion_memoria
    } else {
        &b.tramitacion
    };
    let (pct, txt) = por_pem(tramos, e.pem);
    let suelo_viv = b.minimo_vivienda * n_viv;
    let (mut honorario, mut regla) = if suelo_viv > pct && suelo_viv >= minimo {
        (
            suelo_viv,
            format!("mínimo por vivienda: {viviendas} × {}", eur(b.minimo_vivienda)),
        )
    } else if minimo > pct {
        (minimo, format!("mínimo por expediente {}", eur(minimo)))
    } else {
        (pct, format!("{txt} del PEM"))
    };
    if e.ajeno && !b.recargo_ajeno.is_zero() {
        honorario = redondear(honorario * (CIEN + b.recargo_ajeno) / CIEN, 2);
        regla.push_str(&format!(", +{} % por proyecto de otro técnico", corto(b.recargo_ajeno)));
    }
    let (titulo, que, reparto) = if proyecto {
        (
            "Tramitación y dirección de la instalación",
            "tramitación, as-built y dirección de la instalación",
            &b.reparto_tramitacion,
        )
    } else {
        (
            "Elaboración y tramitación de la memoria técnica",
            "memoria técnica, as-built y tramitación",
            &b.reparto_memoria,
        )
    };
    c.bloques.push(Bloque {
        titulo: titulo.into(),
        regla: format!("Honorario {} ({regla})", eur(honorario)),
        lineas: horas_de(b, que, honorario, reparto),
    });

    // 2. Redacción del proyecto propio
    if proyecto && !e.ajeno && !b.redaccion.is_empty() {
        let (pct, txt) = por_pem(&b.redaccion, e.pem);
        let (h, regla) = if b.redaccion_minimo > pct {
            (b.redaccion_minimo, format!("mínimo {}", eur(b.redaccion_minimo)))
        } else {
            (pct, format!("{txt} del PEM"))
        };
        c.bloques.push(Bloque {
            titulo: "Redacción del proyecto".into(),
            regla: format!("Honorario {} ({regla})", eur(h)),
            lineas: horas_de(b, "redacción del proyecto", h, &b.reparto_redaccion),
        });
    }

    // 3. Justificaciones
    if e.disciplina == Disciplina::Rite && e.justificar_rite {
        let pct = redondear(e.pem * b.justif_rite_pct / CIEN, 2);
        let (h, regla) = if b.justif_rite_minimo > pct {
            (b.justif_rite_minimo, format!("mínimo {}", eur(b.justif_rite_minimo)))
        } else {
            (pct, format!("{} % del PEM", corto(b.justif_rite_pct)))
        };
        c.bloques.push(Bloque {
            titulo: "Justificación RITE".into(),
            regla: format!("Honorario {} ({regla})", eur(h)),
            lineas: horas_de(b, "justificación RITE", h, &b.reparto_justificacion),
        });
    }
    let mut rep_if = Vec::new();
    if e.circuitos_rsif > 0 {
        let modelos = if e.modelos_rsif == 0 {
            e.circuitos_rsif
        } else {
            e.modelos_rsif.min(e.circuitos_rsif)
        };
        let repetidos = e.circuitos_rsif - modelos;
        let p_rep = if b.rsif_repetido.is_zero() {
            b.rsif_circuito
        } else {
            b.rsif_repetido
        };
        let importe = d(modelos) * b.rsif_circuito + d(repetidos) * p_rep;
        let (h, regla) = if b.rsif_minimo > importe {
            (b.rsif_minimo, format!("mínimo {}", eur(b.rsif_minimo)))
        } else {
            (
                importe,
                format!(
                    "{modelos} modelos × {} + {repetidos} repetidos × {}",
                    eur(b.rsif_circuito),
                    eur(p_rep)
                ),
            )
        };
        c.bloques.push(Bloque {
            titulo: format!("Justificación RSIF ({} circuitos)", e.circuitos_rsif),
            regla: format!("Honorario {} ({regla})", eur(h)),
            lineas: horas_de(b, "justificación RSIF", h, &b.reparto_justificacion),
        });
        if e.repercutir
            && let Some(nivel) = e.registro_if
        {
            let (txt, eici) = match nivel {
                RegistroIf::MemoriaA2l => ("memoria nivel 1 (A2L)", b.eici_if_memoria_a2l),
                RegistroIf::MemoriaL1 => ("memoria nivel 2 (L1)", b.eici_if_memoria_l1),
                RegistroIf::Proyecto => ("proyecto nivel 2 sin sala", b.eici_if_proyecto),
                RegistroIf::ProyectoConSala => ("proyecto nivel 2 con sala", b.eici_if_proyecto_sala),
            };
            rep_if.push(importe_fijo(
                format!("EICI registro instalación frigorífica, {txt}"),
                eici,
                Tipo::Repercutido,
            ));
            if !b.tasa_if.is_zero() {
                rep_if.push(importe_fijo(
                    "Tasa de registro de instalación frigorífica".into(),
                    b.tasa_if,
                    Tipo::Repercutido,
                ));
            }
        }
    }

    // 4. Visitas de la OCA
    let visitas_oca = u32::from(proyecto) + e.segundas_visitas;
    if visitas_oca > 0 && !b.oca_horas.is_zero() {
        c.bloques.push(Bloque {
            titulo: format!("Visitas de inspección de la OCA ({visitas_oca})"),
            regla: format!("{} h por visita", corto(b.oca_horas)),
            lineas: vec![por_unidad(
                format!(
                    "{}: preparación y asistencia a la visita de la OCA",
                    Perfil::Tecnico.nombre()
                ),
                "h",
                b.oca_horas * d(visitas_oca),
                b.coste_tecnico,
                Tipo::ManoObra,
            )],
        });
    }

    // 5. Desplazamientos
    let viajes = e.visitas_obra + visitas_oca;
    if viajes > 0 {
        let km = if e.km > Decimal::ZERO { e.km } else { b.viaje_km };
        let horas = if e.horas_viaje > Decimal::ZERO {
            e.horas_viaje
        } else {
            b.viaje_horas
        };
        let v = d(viajes);
        let mut lineas = Vec::new();
        if !horas.is_zero() {
            lineas.push(por_unidad(
                format!("{}: horas de viaje", Perfil::Tecnico.nombre()),
                "h",
                horas * v,
                b.coste_tecnico,
                Tipo::ManoObra,
            ));
        }
        if !km.is_zero() && !b.km.is_zero() {
            lineas.push(por_unidad("Kilometraje".into(), "km", km * v, b.km, Tipo::Indirecto));
        }
        if !b.aparcamiento.is_zero() {
            lineas.push(por_unidad(
                "Aparcamiento".into(),
                "ud",
                v,
                b.aparcamiento,
                Tipo::Indirecto,
            ));
        }
        if km > b.km_umbral && !b.dieta.is_zero() {
            lineas.push(por_unidad("Dietas".into(), "ud", v, b.dieta, Tipo::Indirecto));
        }
        if !lineas.is_empty() {
            c.bloques.push(Bloque {
                titulo: format!("Desplazamientos ({viajes} viajes)"),
                regla: format!(
                    "{} visitas de obra + {visitas_oca} de la OCA; {} km y {} h por viaje",
                    e.visitas_obra,
                    corto(km),
                    corto(horas)
                ),
                lineas,
            });
        }
    }

    // 6. Costes indirectos, gestión y beneficio
    let base: Decimal = c.lineas().map(Linea::importe).sum();
    if !b.margen.is_zero() {
        let precio = redondear(base / CIEN, 3);
        c.bloques.push(Bloque {
            titulo: "Costes indirectos, gestión y beneficio".into(),
            regla: format!("{} % sobre {}", corto(b.margen), eur(base)),
            lineas: vec![Linea {
                resumen: "Costes indirectos, gestión y beneficio de ingeniería".into(),
                unidad: "%".into(),
                cantidad: b.margen,
                precio,
                tipo: Tipo::Margen,
            }],
        });
    }

    // 7. Repercutidos
    if e.repercutir {
        let mut lineas = repercutidos(b, e, &mut c.avisos);
        if e.segundas_visitas > 0 && !b.eici_segunda_visita.is_zero() {
            lineas.push(por_unidad(
                "EICI segunda visita o posterior".into(),
                "ud",
                d(e.segundas_visitas),
                b.eici_segunda_visita,
                Tipo::Repercutido,
            ));
        }
        lineas.extend(rep_if);
        if !lineas.is_empty() {
            c.bloques.push(Bloque {
                titulo: "Tasas, inspección y visados (repercutidos sin margen)".into(),
                regla: "Tablas oficiales del año".into(),
                lineas,
            });
        }
    }

    for l in c.bloques.iter().flat_map(|b| &b.lineas) {
        match l.tipo {
            Tipo::ManoObra => {
                c.coste += l.importe();
                if l.unidad == "h" {
                    c.horas += l.cantidad;
                }
            }
            Tipo::Indirecto => c.coste += l.importe(),
            Tipo::Margen => c.margen += l.importe(),
            Tipo::Repercutido => c.repercutidos += l.importe(),
        }
    }
    if e.pem <= Decimal::ZERO {
        c.avisos
            .push("El PEM de la disciplina es cero: los honorarios quedan en el mínimo.".into());
    }

    c.resumen = format!("Legalización de {} mediante {doc}", e.disciplina.nombre());
    let tasas = if e.repercutir {
        if proyecto {
            ", con tasas, inspección de la EICI y visados"
        } else {
            ", con tasas e inspección de la EICI"
        }
    } else {
        ""
    };
    c.texto = match (e.disciplina, proyecto) {
        (Disciplina::Rite, true) => format!(
            "Legalización de las instalaciones térmicas (RITE) mediante proyecto{}. Incluye dirección de la \
             instalación y certificado final, planos as-built, cálculos justificativos, libro de uso y \
             mantenimiento, cumplimentación de la documentación 3.1.5, aportación del CIE del instalador \
             eléctrico, asistencia a la inspección de la OCA, desplazamientos y gestión del expediente ante \
             el órgano competente de la Comunidad Autónoma{tasas}.",
            if e.ajeno {
                " de otro técnico"
            } else {
                ", incluida su redacción"
            }
        ),
        (Disciplina::Rite, false) => format!(
            "Legalización de las instalaciones térmicas (RITE) mediante memoria técnica. Incluye elaboración \
             de la memoria con la justificación del RITE y, en su caso, del RSIF, comprobación en obra, planos as-built, libro de uso \
             y mantenimiento, cumplimentación de la documentación 3.1.5, aportación del CIE del instalador \
             eléctrico, desplazamientos y gestión del expediente ante el órgano competente de la Comunidad \
             Autónoma{tasas}."
        ),
        (Disciplina::Pci, true) => format!(
            "Legalización de las instalaciones de protección contra incendios (RIPCI) mediante proyecto{}. \
             Incluye dirección de la instalación y certificado final, planos as-built, cálculos \
             justificativos, manual de uso y mantenimiento, asistencia a la inspección de la OCA, \
             desplazamientos y gestión del expediente ante el órgano competente de la Comunidad \
             Autónoma{tasas}.",
            if e.ajeno {
                " de otro técnico"
            } else {
                ", incluida su redacción"
            }
        ),
        (Disciplina::Pci, false) => format!(
            "Legalización de las instalaciones de protección contra incendios (RIPCI) mediante memoria \
             técnica. Incluye elaboración de la memoria, comprobación en obra, planos as-built, manual de \
             uso y mantenimiento, desplazamientos y gestión del expediente ante el órgano competente de la \
             Comunidad Autónoma{tasas}."
        ),
    };
    c
}

fn repercutidos(b: &Baremo, e: &Expediente, avisos: &mut Vec<String>) -> Vec<Linea> {
    let mut r = Vec::new();
    let proyecto = e.documento == Documento::Proyecto;
    let viviendas = e.n_viviendas();
    let n_viv = d(viviendas);
    let fijo = |r: &mut Vec<Linea>, txt: String, v: Decimal| {
        if !v.is_zero() {
            r.push(importe_fijo(txt, v, Tipo::Repercutido));
        }
    };
    match (e.disciplina, proyecto) {
        (Disciplina::Rite, true) => {
            match escalon(&b.tasa_rite_proyecto, e.pem) {
                Some(v) => fijo(
                    &mut r,
                    format!("Tasa de registro RITE, proyecto (inversión {})", eur(e.pem)),
                    v,
                ),
                None => avisos.push("La tabla de tasas RITE de proyecto no cubre este PEM.".into()),
            }
            if viviendas > 0 && e.potencia_kw < d(70) {
                let v = escalon(&b.eici_rite_viviendas, n_viv)
                    .unwrap_or(b.eici_rite_viviendas_base + b.eici_rite_por_vivienda * n_viv);
                fijo(
                    &mut r,
                    format!("EICI proyecto RITE, edificio de {viviendas} viviendas"),
                    v,
                );
            } else {
                if e.potencia_kw < d(70) {
                    avisos.push("Proyecto RITE con menos de 70 kW: revise si basta con memoria técnica.".into());
                }
                let ultimo = b.eici_rite_kw.last().and_then(|tr| tr.hasta);
                let v = match escalon(&b.eici_rite_kw, e.potencia_kw) {
                    Some(v) => v,
                    None => {
                        let tope = ultimo.unwrap_or(Decimal::ZERO);
                        let base = b.eici_rite_kw.last().map_or(Decimal::ZERO, |tr| tr.valor);
                        base + b.eici_rite_por_100kw * ((e.potencia_kw - tope) / CIEN).ceil()
                    }
                };
                fijo(&mut r, format!("EICI proyecto RITE ({} kW)", num(e.potencia_kw, 0)), v);
            }
            if e.primario_frio {
                fijo(
                    &mut r,
                    "EICI coste adicional por primario frigorífico".into(),
                    b.eici_rite_primario,
                );
            }
        }
        (Disciplina::Rite, false) => {
            if viviendas > 0 {
                if !b.tasa_rite_memoria_vivienda.is_zero() {
                    r.push(por_unidad(
                        "Tasa de registro RITE, memoria de vivienda".into(),
                        "ud",
                        n_viv,
                        b.tasa_rite_memoria_vivienda,
                        Tipo::Repercutido,
                    ));
                }
                let eici = if e.primario_frio {
                    b.eici_rite_memoria_primario
                } else {
                    b.eici_rite_memoria
                };
                if !eici.is_zero() {
                    r.push(por_unidad(
                        "EICI memoria individual de vivienda".into(),
                        "ud",
                        n_viv,
                        eici,
                        Tipo::Repercutido,
                    ));
                }
            } else {
                fijo(
                    &mut r,
                    "Tasa de registro RITE, memoria (no vivienda)".into(),
                    b.tasa_rite_memoria_otro,
                );
                let eici = if e.primario_frio {
                    b.eici_rite_memoria_primario
                } else {
                    b.eici_rite_memoria
                };
                fijo(&mut r, "EICI memoria técnica".into(), eici);
            }
        }
        (Disciplina::Pci, true) => {
            let v = escalon(&b.tasa_pci, e.pem).unwrap_or_else(|| {
                if b.tasa_pci_tam_bloque.is_zero() {
                    Decimal::ZERO
                } else {
                    b.tasa_pci_bloque * (e.pem / b.tasa_pci_tam_bloque).ceil()
                }
            });
            fijo(
                &mut r,
                format!("Tasa de registro PCI, proyecto (inversión {})", eur(e.pem)),
                v,
            );
            let fila = (e.instalaciones_pci.max(1) as usize - 1).min(b.eici_pci_proyecto.len().saturating_sub(1));
            let col = b
                .eici_pci_superficies
                .iter()
                .position(|&s| e.superficie < s)
                .unwrap_or(b.eici_pci_superficies.len());
            match b
                .eici_pci_proyecto
                .get(fila)
                .and_then(|f| f.get(col.min(f.len().saturating_sub(1))))
            {
                Some(&v) => fijo(
                    &mut r,
                    format!(
                        "EICI proyecto PCI ({} instalaciones, {} m²)",
                        e.instalaciones_pci,
                        num(e.superficie, 0)
                    ),
                    v,
                ),
                None => avisos.push("Falta la tabla EICI de proyectos PCI en el baremo.".into()),
            }
            if e.presion_diferencial {
                fijo(
                    &mut r,
                    "EICI coste adicional por sistema de presión diferencial".into(),
                    b.eici_pci_presion,
                );
            }
        }
        (Disciplina::Pci, false) => {
            fijo(&mut r, "Tasa de registro PCI, memoria".into(), b.tasa_pci_memoria);
            let i = (e.instalaciones_pci.max(1) as usize - 1).min(b.eici_pci_memoria.len().saturating_sub(1));
            match b.eici_pci_memoria.get(i) {
                Some(&v) => fijo(
                    &mut r,
                    format!("EICI memoria PCI ({} instalaciones)", e.instalaciones_pci),
                    v,
                ),
                None => avisos.push("Falta la tabla EICI de memorias PCI en el baremo.".into()),
            }
        }
    }
    if proyecto {
        let visado: Decimal = por_tramos(&b.visado, e.pem)
            .iter()
            .map(|(_, _, k, parte)| *parte * *k)
            .sum();
        fijo(
            &mut r,
            format!("Derechos de visado del proyecto (PEM {})", eur(e.pem)),
            visado,
        );
        if b.visado_direccion_obra {
            fijo(
                &mut r,
                "Derechos de visado del certificado de dirección de obra".into(),
                visado,
            );
        }
    }
    r
}

// ------------------------------------------------------- en el presupuesto

/// Código de la partida de una disciplina dentro del capítulo `LEG`.
pub fn codigo_partida(disciplina: Disciplina) -> String {
    format!("{CAPITULO}-{}", disciplina.sigla())
}

/// Importe de cada capítulo de primer nivel (para elegir la base del PEM),
/// sin el de legalización.
pub fn capitulos(p: &Presupuesto) -> Result<Vec<(String, String, Decimal)>, ErrorMotor> {
    let v = p.valorar()?;
    let raiz = p.concepto(&p.raiz)?;
    Ok(raiz
        .descomposicion
        .iter()
        .filter(|l| l.hijo != CAPITULO)
        .filter_map(|l| {
            let c = p.conceptos.get(&l.hijo)?;
            let precio = v.precios.get(&l.hijo).copied().unwrap_or_default();
            Some((c.codigo.clone(), c.resumen.clone(), redondear(precio * l.cantidad(), 2)))
        })
        .collect())
}

/// Escribe (o sustituye) la partida `LEG-<disciplina>` en el capítulo `LEG`
/// con todo su descompuesto. Si algo falla, el presupuesto queda como estaba.
pub fn aplicar(p: &mut Presupuesto, disciplina: Disciplina, calculo: &Calculo) -> Result<(), ErrorMotor> {
    let copia = p.clone();
    let r = aplicar_sin_deshacer(p, disciplina, calculo);
    if r.is_err() {
        *p = copia;
    }
    r
}

fn aplicar_sin_deshacer(p: &mut Presupuesto, disciplina: Disciplina, calculo: &Calculo) -> Result<(), ErrorMotor> {
    let raiz = p.raiz.clone();
    if !p.conceptos.contains_key(CAPITULO) {
        p.nuevo_capitulo(&raiz, CAPITULO, "LEGALIZACIÓN")?;
    }
    let codigo = codigo_partida(disciplina);
    if p.conceptos.contains_key(&codigo) {
        for padre in p.padres(&codigo) {
            p.quitar_y_purgar(&padre, &codigo)?;
        }
    }
    let mut lineas = Vec::new();
    for (i, l) in calculo.lineas().enumerate() {
        let rc = if l.tipo == Tipo::Margen {
            format!("{codigo}%CI")
        } else {
            format!("{codigo}-{:02}", i + 1)
        };
        let concepto = if l.tipo == Tipo::Margen {
            Concepto::porcentaje(&rc, &l.resumen)
        } else {
            Concepto::basico(&rc, &l.unidad, &l.resumen, l.naturaleza(), l.precio)
        };
        p.insertar(concepto)?;
        lineas.push(LineaDescomposicion::new(rc, l.cantidad));
    }
    let mut c = Concepto::partida(&codigo, "ud", &calculo.resumen, lineas);
    c.texto = Some(calculo.texto.clone());
    p.insertar(c)?;
    p.anadir_linea(CAPITULO, &codigo, Decimal::ONE)?;
    p.validar()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn b() -> Baremo {
        Baremo::ejemplo(2026)
    }

    fn bloque<'a>(c: &'a Calculo, titulo: &str) -> Option<&'a Bloque> {
        c.bloques.iter().find(|b| b.titulo.starts_with(titulo))
    }

    /// El honorario de baremo se reproduce (al céntimo de redondeo) con
    /// horas a coste + margen.
    fn honorario_bloque(c: &Calculo, titulo: &str) -> Decimal {
        let coste = bloque(c, titulo).unwrap().importe();
        redondear(coste * (CIEN + b().margen) / CIEN, 0)
    }

    #[test]
    fn tramos_acumulativos_en_horas_por_perfil() {
        // Proyecto: 1 % hasta 100.000 y 0,5 % del exceso: 150.000 → 1.250.
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(150000));
        let c = calcular(&b(), &e);
        let tram = bloque(&c, "Tramitación").unwrap();
        assert!(tram.regla.contains("1.250,00"));
        assert_eq!(tram.lineas.len(), 4); // senior, junior, técnico, documentación
        assert!(tram.lineas.iter().all(|l| l.unidad == "h" && l.tipo == Tipo::ManoObra));
        assert_eq!(honorario_bloque(&c, "Tramitación"), dec!(1250));
    }

    #[test]
    fn memorias_con_tramos_reparto_y_texto_propios() {
        // Memoria: 1,5 % hasta 50.000 y 0,75 % del exceso: 150.000 → 1.500.
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(150000));
        assert_eq!(e.visitas_obra, 1);
        let c = calcular(&b(), &e);
        let m = bloque(&c, "Elaboración y tramitación de la memoria").unwrap();
        assert!(m.regla.contains("1.500,00"));
        assert_eq!(honorario_bloque(&c, "Elaboración"), dec!(1500));
        // Reparto propio: 10 % senior.
        let horas: Decimal = m.lineas.iter().map(|l| l.cantidad).sum();
        assert_eq!(redondear(m.lineas[0].cantidad / horas * CIEN, 0), dec!(10));
        assert!(!c.texto.contains("dirección de la instalación"));
        assert!(c.texto.contains("comprobación en obra"));
        // Proyecto: sus tramos, su reparto y la dirección de la instalación.
        e.documento = Documento::Proyecto;
        let c = calcular(&b(), &e);
        assert!(bloque(&c, "Tramitación y dirección").is_some());
        assert!(c.texto.contains("dirección de la instalación"));
        // Sin tramos de memoria se usan los de proyecto.
        let mut bm = b();
        bm.tramitacion_memoria.clear();
        e.documento = Documento::Memoria;
        assert_eq!(honorario_bloque(&calcular(&bm, &e), "Elaboración"), dec!(1250));
    }

    #[test]
    fn minimos_de_memoria_proyecto_y_vivienda() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(20000));
        assert_eq!(honorario_bloque(&calcular(&b(), &e), "Elaboración"), dec!(300));
        e.documento = Documento::Proyecto;
        assert_eq!(honorario_bloque(&calcular(&b(), &e), "Tramitación"), dec!(500));
        e.documento = Documento::Memoria;
        e.pem = dec!(80000);
        e.viviendas = 40;
        // En terciario el nº de viviendas no cuenta: 1,5 % de 50.000 + 0,75 % de 30.000.
        assert_eq!(honorario_bloque(&calcular(&b(), &e), "Elaboración"), dec!(975));
        e.uso = Uso::Viviendas;
        let c = calcular(&b(), &e);
        assert_eq!(honorario_bloque(&c, "Elaboración"), dec!(2000));
        // Repercutidos por vivienda: tasa 3 + EICI 60, sin margen.
        assert_eq!(c.repercutidos, dec!(40) * dec!(63));
    }

    #[test]
    fn proyecto_propio_redaccion_y_ajeno_recargo() {
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(10000));
        let c = calcular(&b(), &e);
        assert_eq!(honorario_bloque(&c, "Redacción"), dec!(600));
        e.ajeno = true;
        let c = calcular(&b(), &e);
        assert!(bloque(&c, "Redacción").is_none());
        assert_eq!(honorario_bloque(&c, "Tramitación"), dec!(600)); // 500 + 20 %
    }

    #[test]
    fn visitas_oca_y_desplazamientos_a_coste_con_margen() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(50000));
        e.potencia_kw = dec!(120);
        e.segundas_visitas = 1;
        let c = calcular(&b(), &e);
        // 2 visitas OCA × 2 h × 40.
        assert_eq!(bloque(&c, "Visitas").unwrap().importe(), dec!(160));
        // 4 viajes (2 de obra + 2 OCA): 4 h × 40 + 160 km × 0,25 + 4 × 10; sin dietas.
        assert_eq!(
            bloque(&c, "Desplazamientos").unwrap().importe(),
            dec!(160) + dec!(40) + dec!(40)
        );
        // Lejos: dietas.
        e.km = dec!(250);
        e.horas_viaje = dec!(3);
        let c = calcular(&b(), &e);
        let desp = bloque(&c, "Desplazamientos").unwrap();
        assert_eq!(
            desp.importe(),
            dec!(12) * dec!(40) + dec!(1000) * dec!(0.25) + dec!(40) + dec!(120)
        );
        // Margen: 25 % sobre todo lo que no es repercutido.
        let base: Decimal = c
            .lineas()
            .filter(|l| matches!(l.tipo, Tipo::ManoObra | Tipo::Indirecto))
            .map(Linea::importe)
            .sum();
        assert_eq!(c.coste, base);
        assert_eq!(c.margen, redondear(redondear(base / CIEN, 3) * dec!(25), 2));
        // La EICI de segunda visita se repercute sin margen.
        let rep = bloque(&c, "Tasas").unwrap();
        assert!(
            rep.lineas
                .iter()
                .any(|l| l.resumen.contains("segunda visita") && l.importe() == dec!(300))
        );
    }

    #[test]
    fn memoria_rite_lleva_justificacion_rite_y_rsif() {
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(7000));
        assert!(e.justificar_rite && e.circuitos_rsif == 1);
        let c = calcular(&b(), &e);
        assert_eq!(honorario_bloque(&c, "Justificación RITE"), dec!(200)); // mínimo
        assert_eq!(honorario_bloque(&c, "Justificación RSIF"), dec!(100)); // 1 modelo
        let pci = Expediente::nuevo(Disciplina::Pci, Documento::Memoria, dec!(7000));
        assert!(!pci.justificar_rite && pci.circuitos_rsif == 0);
        let mut p = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(7000));
        assert!(!p.justificar_rite_por_defecto());
        p.ajeno = true;
        assert!(p.justificar_rite_por_defecto());
    }

    #[test]
    fn memoria_sin_visitas_oca() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(30000));
        e.visitas_obra = 0;
        let c = calcular(&b(), &e);
        assert!(bloque(&c, "Visitas").is_none());
        assert!(bloque(&c, "Desplazamientos").is_none());
        assert!(!c.lineas().any(|l| l.resumen.contains("visado")));
    }

    #[test]
    fn repercutidos_de_proyecto() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(200000));
        e.potencia_kw = dec!(250);
        let c = calcular(&b(), &e);
        // Tasa 50 + EICI 800 + visado (200 + 100) × 2.
        assert_eq!(c.repercutidos, dec!(50) + dec!(800) + dec!(600));
        e.potencia_kw = dec!(1250); // 800 + 10 × ceil(250/100)
        let c = calcular(&b(), &e);
        assert!(
            c.lineas()
                .any(|l| l.resumen.starts_with("EICI proyecto") && l.importe() == dec!(830))
        );
        // PCI: tasa 10 × 30 bloques, EICI fila 2 col 2, presión, visado (200+50) × 2.
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(150000));
        e.instalaciones_pci = 5;
        e.superficie = dec!(2000);
        e.presion_diferencial = true;
        assert_eq!(
            calcular(&b(), &e).repercutidos,
            dec!(300) + dec!(550) + dec!(150) + dec!(500)
        );
    }

    #[test]
    fn rsif_por_modelo_y_repetido_con_registro() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(50000));
        e.circuitos_rsif = 40;
        e.modelos_rsif = 2;
        e.registro_if = Some(RegistroIf::MemoriaA2l);
        let c = calcular(&b(), &e);
        assert_eq!(honorario_bloque(&c, "Justificación RSIF"), dec!(200) + dec!(760));
        assert!(
            c.lineas()
                .any(|l| l.tipo == Tipo::Repercutido && l.importe() == dec!(120))
        );
        assert!(
            c.lineas()
                .any(|l| l.tipo == Tipo::Repercutido && l.importe() == dec!(35))
        );
    }

    #[test]
    fn aplicar_crea_una_partida_con_todo_el_descompuesto() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        let pem_antes = p.pem().unwrap();
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, pem_antes);
        e.potencia_kw = dec!(80);
        e.segundas_visitas = 1;
        let c = calcular(&b(), &e);
        aplicar(&mut p, Disciplina::Rite, &c).unwrap();
        // El presupuesto valora la partida igual que el cálculo (también el %).
        assert_eq!(p.pem().unwrap(), pem_antes + c.total());
        aplicar(&mut p, Disciplina::Rite, &c).unwrap();
        assert_eq!(p.pem().unwrap(), pem_antes + c.total());
        assert_eq!(p.concepto(CAPITULO).unwrap().descomposicion.len(), 1);
        let suma: Decimal = capitulos(&p).unwrap().iter().map(|x| x.2).sum();
        assert_eq!(suma, pem_antes);
        let partida = p.concepto("LEG-RITE").unwrap();
        assert_eq!(partida.descomposicion.len(), c.lineas().count());
        assert!(p.concepto("LEG-RITE%CI").unwrap().naturaleza == Naturaleza::Porcentaje);
        assert!(partida.texto.as_deref().unwrap().contains("as-built"));
        // Las horas son mano de obra en el presupuesto.
        let h = &partida.descomposicion[0].hijo;
        assert_eq!(p.concepto(h).unwrap().naturaleza, Naturaleza::ManoObra);
    }

    #[test]
    fn baremo_por_anio() {
        let mut bs = Baremos::default();
        bs.del_anio(2026).tramitacion_minimo = dec!(750);
        assert_eq!(bs.vigente(2027).unwrap().anio, 2026);
        let b27 = bs.del_anio(2027);
        assert_eq!(b27.tramitacion_minimo, dec!(750)); // copia del anterior
        b27.tramitacion_minimo = dec!(800);
        assert_eq!(bs.vigente(2027).unwrap().tramitacion_minimo, dec!(800));
        assert_eq!(bs.vigente(2026).unwrap().tramitacion_minimo, dec!(750));
        let json = serde_json::to_string(&bs).unwrap();
        let de: Baremos = serde_json::from_str(&json).unwrap();
        assert_eq!(de, bs);
        // Campos que faltan toman su valor por defecto (y los desconocidos se ignoran).
        let parcial: Baremos =
            serde_json::from_str(r#"{"versiones":[{"anio":2026,"km":"0.26","visita_oca":"200"}]}"#).unwrap();
        assert_eq!(parcial.versiones[0].km, dec!(0.26));
        assert!(parcial.versiones[0].visado_direccion_obra);
        assert_eq!(parcial.versiones[0].reparto_tramitacion.tecnico, dec!(30));
    }
}
