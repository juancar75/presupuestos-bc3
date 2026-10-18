// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-legalizacion
//!
//! Costes de legalización de una obra (RITE, PCI y RSIF) como partidas
//! descompuestas dentro del presupuesto de la instaladora:
//!
//! - **Coste de tramitación** (honorarios propios): % del PEM de la
//!   disciplina por tramos acumulativos, con un mínimo por expediente y otro
//!   por vivienda; +x % si el proyecto es de otro técnico. Incluye as-built,
//!   cálculos, libro de uso y mantenimiento, documentación 3.1.5, CIE del
//!   instalador eléctrico, dirección de obra y gestión del expediente.
//! - **Visita de la OCA** (proyectos) y **segundas visitas**.
//! - **Justificación RITE** (% del PEM con mínimo) y **justificación RSIF**
//!   (por circuito con mínimo), con registro de instalación frigorífica opcional.
//! - **Desplazamiento** (€/km y horas de viaje) por encima de un umbral.
//! - **Repercutidos**: tasas de la Comunidad, inspección EICI y derechos de
//!   visado (solo proyectos), según las tablas del año.
//!
//! El baremo (honorarios y tablas) es del usuario y vive fuera del
//! repositorio, en `legalizacion.json` de la carpeta de configuración, con una
//! versión por año. [`Baremo::ejemplo`] lleva valores **sintéticos** para
//! pruebas y como plantilla.

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

/// Honorarios y tablas de un año. Todos los importes sin IVA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Baremo {
    pub anio: i32,
    /// Procedencia de los datos (p. ej. «Tablas PRMEM 2026, honorarios AIDESEGI»).
    pub nota: String,

    // --- honorarios propios
    /// Tramitación: % del PEM de la disciplina por tramos acumulativos.
    pub tramitacion: Vec<Tramo>,
    /// Mínimo por expediente en proyectos (y en memorias si el de memoria es cero).
    pub tramitacion_minimo: Decimal,
    /// Mínimo por expediente en memorias técnicas (0 = el de proyectos).
    pub tramitacion_minimo_memoria: Decimal,
    /// Redacción del proyecto cuando es propio: % del PEM por tramos acumulativos.
    pub redaccion: Vec<Tramo>,
    pub redaccion_minimo: Decimal,
    /// Mínimo por vivienda en bloques (se cobra el mayor).
    pub minimo_vivienda: Decimal,
    /// Recargo (%) cuando el proyecto es de otro técnico.
    pub recargo_ajeno: Decimal,
    pub visita_oca: Decimal,
    pub segunda_visita: Decimal,
    /// Justificación RITE: % del PEM RITE con mínimo.
    pub justif_rite_pct: Decimal,
    pub justif_rite_minimo: Decimal,
    /// Justificación RSIF: por circuito de cada modelo distinto, por circuito
    /// repetido (0 = como uno distinto) y mínimo.
    pub rsif_circuito: Decimal,
    pub rsif_repetido: Decimal,
    pub rsif_minimo: Decimal,
    /// Desplazamiento: €/km (ida y vuelta) y €/h de viaje, solo por encima del umbral de km.
    pub km: Decimal,
    pub km_umbral: Decimal,
    pub hora_viaje: Decimal,
    /// Tarifa horaria media para traducir los honorarios a horas (comprobación).
    pub hora_referencia: Decimal,

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
    /// EICI de proyecto para bloques de N viviendas sin sala de máquinas (escalón);
    /// por encima, `base + por_vivienda × N`.
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
        Self {
            anio: 0,
            nota: String::new(),
            tramitacion: Vec::new(),
            tramitacion_minimo: Decimal::ZERO,
            tramitacion_minimo_memoria: Decimal::ZERO,
            redaccion: Vec::new(),
            redaccion_minimo: Decimal::ZERO,
            minimo_vivienda: Decimal::ZERO,
            recargo_ajeno: Decimal::ZERO,
            visita_oca: Decimal::ZERO,
            segunda_visita: Decimal::ZERO,
            justif_rite_pct: Decimal::ZERO,
            justif_rite_minimo: Decimal::ZERO,
            rsif_circuito: Decimal::ZERO,
            rsif_repetido: Decimal::ZERO,
            rsif_minimo: Decimal::ZERO,
            km: Decimal::ZERO,
            km_umbral: Decimal::ZERO,
            hora_viaje: Decimal::ZERO,
            hora_referencia: Decimal::ZERO,
            visado: Vec::new(),
            visado_direccion_obra: true,
            tasa_rite_proyecto: Vec::new(),
            tasa_rite_memoria_vivienda: Decimal::ZERO,
            tasa_rite_memoria_otro: Decimal::ZERO,
            eici_rite_memoria: Decimal::ZERO,
            eici_rite_memoria_primario: Decimal::ZERO,
            eici_rite_kw: Vec::new(),
            eici_rite_por_100kw: Decimal::ZERO,
            eici_rite_viviendas: Vec::new(),
            eici_rite_viviendas_base: Decimal::ZERO,
            eici_rite_por_vivienda: Decimal::ZERO,
            eici_rite_primario: Decimal::ZERO,
            eici_segunda_visita: Decimal::ZERO,
            tasa_pci: Vec::new(),
            tasa_pci_bloque: Decimal::ZERO,
            tasa_pci_tam_bloque: Decimal::ZERO,
            tasa_pci_memoria: Decimal::ZERO,
            eici_pci_memoria: Vec::new(),
            eici_pci_proyecto: Vec::new(),
            eici_pci_superficies: Vec::new(),
            eici_pci_presion: Decimal::ZERO,
            eici_if_memoria_a2l: Decimal::ZERO,
            eici_if_memoria_l1: Decimal::ZERO,
            eici_if_proyecto: Decimal::ZERO,
            eici_if_proyecto_sala: Decimal::ZERO,
            tasa_if: Decimal::ZERO,
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
            nota: "VALORES DE EJEMPLO: sustitúyalos por sus honorarios y las tablas oficiales del año".into(),
            tramitacion: vec![t(m(100_000), d(1)), t(None, Decimal::new(5, 1))],
            tramitacion_minimo: d(500),
            tramitacion_minimo_memoria: d(300),
            redaccion: vec![t(m(100_000), Decimal::new(15, 1)), t(None, d(1))],
            redaccion_minimo: d(600),
            minimo_vivienda: d(50),
            recargo_ajeno: d(20),
            visita_oca: d(100),
            segunda_visita: d(100),
            justif_rite_pct: Decimal::new(2, 1),
            justif_rite_minimo: d(200),
            rsif_circuito: d(100),
            rsif_repetido: d(20),
            rsif_minimo: d(200),
            km: Decimal::new(25, 2),
            km_umbral: d(50),
            hora_viaje: d(40),
            hora_referencia: d(40),
            visado: vec![t(m(100_000), Decimal::new(2, 3)), t(None, Decimal::new(1, 3))],
            visado_direccion_obra: true,
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
            tasa_pci_memoria: Decimal::ZERO,
            eici_pci_memoria: vec![d(200), d(250), d(300)],
            eici_pci_proyecto: vec![vec![d(400), d(500)], vec![d(450), d(550)]],
            eici_pci_superficies: vec![d(1000)],
            eici_pci_presion: d(150),
            eici_if_memoria_a2l: d(120),
            eici_if_memoria_l1: d(130),
            eici_if_proyecto: d(250),
            eici_if_proyecto_sala: d(350),
            tasa_if: d(35),
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
    /// PEM de la disciplina (base del % de tramitación y de la tasa).
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
    /// Justificación RITE aparte (solo RITE; en memorias y proyectos propios
    /// suele ir incluida en la tramitación o la redacción).
    pub justificar_rite: bool,
    /// Modelos (circuitos distintos) a justificar por RSIF; el resto de
    /// `circuitos_rsif` son repetidos. 0 = todos distintos.
    pub modelos_rsif: u32,
    /// Circuitos frigoríficos a justificar por RSIF (0 = sin RSIF).
    pub circuitos_rsif: u32,
    pub registro_if: Option<RegistroIf>,
    pub segundas_visitas: u32,
    /// Km de ida y vuelta por viaje y nº de viajes.
    pub km: Decimal,
    pub horas_viaje: Decimal,
    pub viajes: u32,
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
            justificar_rite: false,
            modelos_rsif: 0,
            circuitos_rsif: 0,
            registro_if: None,
            segundas_visitas: 0,
            km: Decimal::ZERO,
            horas_viaje: Decimal::ZERO,
            viajes: 1,
            repercutir: true,
        }
    }
}

// ----------------------------------------------------------------- cálculo

/// Línea del descompuesto de una partida de legalización.
#[derive(Debug, Clone, PartialEq)]
pub struct Linea {
    pub resumen: String,
    pub unidad: String,
    pub naturaleza: Naturaleza,
    pub cantidad: Decimal,
    pub precio: Decimal,
    /// Honorario propio (cuenta para las horas) o importe repercutido.
    pub propia: bool,
}

impl Linea {
    pub fn importe(&self) -> Decimal {
        redondear(self.cantidad * self.precio, 2)
    }
}

fn importe_fijo(resumen: String, precio: Decimal, propia: bool) -> Linea {
    Linea {
        resumen,
        unidad: "ud".into(),
        naturaleza: Naturaleza::Otros,
        cantidad: Decimal::ONE,
        precio: redondear(precio, 2),
        propia,
    }
}

fn por_unidad(resumen: String, unidad: &str, n: Decimal, precio: Decimal, propia: bool) -> Linea {
    Linea {
        resumen,
        unidad: unidad.into(),
        naturaleza: Naturaleza::Otros,
        cantidad: n,
        precio: redondear(precio, 2),
        propia,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartidaLeg {
    /// Sufijo del código (`TRAM`, `OCA`…): la partida es `LEG-<disciplina>-<sufijo>`.
    pub sufijo: &'static str,
    pub resumen: String,
    pub texto: String,
    pub lineas: Vec<Linea>,
}

impl PartidaLeg {
    pub fn importe(&self) -> Decimal {
        self.lineas.iter().map(Linea::importe).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calculo {
    pub partidas: Vec<PartidaLeg>,
    /// Honorarios propios (sin repercutidos).
    pub honorarios: Decimal,
    pub repercutidos: Decimal,
    /// Horas que equivalen los honorarios a la tarifa de referencia.
    pub horas: Option<Decimal>,
    pub avisos: Vec<String>,
}

impl Calculo {
    pub fn total(&self) -> Decimal {
        self.honorarios + self.repercutidos
    }
}

/// Número sin ceros sobrantes («25», «0,7», «0,26»).
fn corto(v: Decimal) -> String {
    let n = v.normalize();
    num(n, n.scale())
}

fn rango(desde: Decimal, hasta: Option<Decimal>) -> String {
    match hasta {
        Some(h) if desde.is_zero() => format!("hasta {}", eur(h)),
        Some(h) => format!("de {} a {}", eur(desde), eur(h)),
        None => format!("más de {}", eur(desde)),
    }
}

/// Líneas «<qué>: x % s/ PEM <tramo>» de una tabla acumulativa.
fn pct_por_tramos(que: &str, tramos: &[Tramo], pem: Decimal) -> Vec<Linea> {
    por_tramos(tramos, pem)
        .into_iter()
        .map(|(desde, hasta, pct, parte)| {
            importe_fijo(
                format!(
                    "{que}: {} % s/ PEM {} ({})",
                    corto(pct),
                    rango(desde, hasta),
                    eur(parte)
                ),
                parte * pct / CIEN,
                true,
            )
        })
        .collect()
}

/// Calcula las partidas de legalización de un expediente con un baremo.
pub fn calcular(b: &Baremo, e: &Expediente) -> Calculo {
    let mut c = Calculo::default();
    let sigla = e.disciplina.sigla();
    let proyecto = e.documento == Documento::Proyecto;
    let doc = if proyecto { "proyecto" } else { "memoria técnica" };
    let viviendas = e.n_viviendas();
    let n_viv = d(viviendas);

    // 1. Tramitación
    let minimo = if !proyecto && !b.tramitacion_minimo_memoria.is_zero() {
        b.tramitacion_minimo_memoria
    } else {
        b.tramitacion_minimo
    };
    let mut lineas = pct_por_tramos("Tramitación", &b.tramitacion, e.pem);
    let por_pem: Decimal = lineas.iter().map(Linea::importe).sum();
    let suelo_viv = b.minimo_vivienda * n_viv;
    if suelo_viv > por_pem && suelo_viv >= minimo {
        lineas = vec![por_unidad(
            format!("Tramitación: mínimo por vivienda ({viviendas} viv.)"),
            "viv",
            n_viv,
            b.minimo_vivienda,
            true,
        )];
    } else if minimo > por_pem {
        lineas = vec![importe_fijo(
            "Tramitación: importe mínimo por expediente".into(),
            minimo,
            true,
        )];
    }
    if e.ajeno && !b.recargo_ajeno.is_zero() {
        let base: Decimal = lineas.iter().map(Linea::importe).sum();
        lineas.push(importe_fijo(
            format!(
                "Revisión y adaptación de proyecto de otro técnico (+{} %)",
                corto(b.recargo_ajeno)
            ),
            base * b.recargo_ajeno / CIEN,
            true,
        ));
    }
    let texto_tram = match e.disciplina {
        Disciplina::Rite => format!(
            "Coste de tramitación de la legalización de las instalaciones térmicas (RITE) mediante {doc}. \
             Incluye planos as-built, cálculos justificativos, libro de uso y mantenimiento, cumplimentación \
             de la documentación 3.1.5, aportación del CIE del instalador eléctrico, dirección de obra y \
             gestión del expediente ante el órgano competente de la Comunidad Autónoma."
        ),
        Disciplina::Pci => format!(
            "Coste de tramitación de la legalización de las instalaciones de protección contra incendios \
             (RIPCI) mediante {doc}. Incluye planos as-built, cálculos justificativos, certificado de la \
             empresa instaladora, manual de uso y mantenimiento, dirección de obra y gestión del expediente \
             ante el órgano competente de la Comunidad Autónoma."
        ),
    };
    c.partidas.push(PartidaLeg {
        sufijo: "TRAM",
        resumen: format!("Coste de tramitación {sigla} ({doc})"),
        texto: texto_tram,
        lineas,
    });

    // 1b. Redacción del proyecto propio
    if proyecto && !e.ajeno && !b.redaccion.is_empty() {
        let mut lineas = pct_por_tramos("Redacción", &b.redaccion, e.pem);
        let por_pem: Decimal = lineas.iter().map(Linea::importe).sum();
        if b.redaccion_minimo > por_pem {
            lineas = vec![importe_fijo(
                "Redacción: importe mínimo".into(),
                b.redaccion_minimo,
                true,
            )];
        }
        c.partidas.push(PartidaLeg {
            sufijo: "RED",
            resumen: format!("Redacción del proyecto {sigla}"),
            texto: format!(
                "Redacción del proyecto de {} firmado por técnico competente: memoria, cálculos y \
                 justificación reglamentaria, pliego, presupuesto y planos.",
                e.disciplina.nombre()
            ),
            lineas,
        });
    }

    // 2. Visitas de la OCA
    if proyecto && !b.visita_oca.is_zero() {
        c.partidas.push(PartidaLeg {
            sufijo: "OCA",
            resumen: format!("Asistencia a la visita de inspección de la OCA ({sigla})"),
            texto: "Preparación y asistencia del técnico a la visita de inspección inicial de la entidad \
                    de control (EICI/OCA) para la puesta en servicio."
                .into(),
            lineas: vec![importe_fijo(
                "Asistencia a la visita de la OCA".into(),
                b.visita_oca,
                true,
            )],
        });
    }
    if e.segundas_visitas > 0 {
        let n = d(e.segundas_visitas);
        let mut lineas = vec![por_unidad(
            "Asistencia a segunda visita o posterior".into(),
            "ud",
            n,
            b.segunda_visita,
            true,
        )];
        if e.repercutir && !b.eici_segunda_visita.is_zero() {
            lineas.push(por_unidad(
                "Tarifa EICI de segunda visita o posterior (repercutida)".into(),
                "ud",
                n,
                b.eici_segunda_visita,
                false,
            ));
        }
        c.partidas.push(PartidaLeg {
            sufijo: "VIS2",
            resumen: format!("Segundas visitas de inspección ({sigla})"),
            texto: "Visitas adicionales de la entidad de control por defectos o trabajos pendientes.".into(),
            lineas,
        });
    }

    // 3. Justificaciones
    if e.disciplina == Disciplina::Rite && e.justificar_rite {
        let por_pct = redondear(e.pem * b.justif_rite_pct / CIEN, 2);
        let linea = if b.justif_rite_minimo > por_pct {
            importe_fijo("Justificación RITE: importe mínimo".into(), b.justif_rite_minimo, true)
        } else {
            importe_fijo(
                format!(
                    "Justificación RITE: {} % s/ PEM ({})",
                    corto(b.justif_rite_pct),
                    eur(e.pem)
                ),
                por_pct,
                true,
            )
        };
        c.partidas.push(PartidaLeg {
            sufijo: "JRITE",
            resumen: "Justificación del cumplimiento del RITE".into(),
            texto: "Justificación de las exigencias de bienestar e higiene, eficiencia energética y seguridad \
                    del RITE (IT 1) de la instalación ejecutada."
                .into(),
            lineas: vec![linea],
        });
    }
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
        let mut lineas = if b.rsif_minimo > importe {
            vec![importe_fijo(
                format!("Justificación RSIF: importe mínimo ({} circuitos)", e.circuitos_rsif),
                b.rsif_minimo,
                true,
            )]
        } else {
            let mut v = vec![por_unidad(
                "Justificación RSIF por circuito de cada modelo".into(),
                "circ",
                d(modelos),
                b.rsif_circuito,
                true,
            )];
            if repetidos > 0 {
                v.push(por_unidad(
                    "Justificación RSIF por circuito repetido".into(),
                    "circ",
                    d(repetidos),
                    p_rep,
                    true,
                ));
            }
            v
        };
        if e.repercutir
            && let Some(nivel) = e.registro_if
        {
            let (txt, eici) = match nivel {
                RegistroIf::MemoriaA2l => ("memoria nivel 1 (A2L)", b.eici_if_memoria_a2l),
                RegistroIf::MemoriaL1 => ("memoria nivel 2 (L1)", b.eici_if_memoria_l1),
                RegistroIf::Proyecto => ("proyecto nivel 2 sin sala", b.eici_if_proyecto),
                RegistroIf::ProyectoConSala => ("proyecto nivel 2 con sala", b.eici_if_proyecto_sala),
            };
            lineas.push(importe_fijo(
                format!("EICI registro instalación frigorífica, {txt} (repercutida)"),
                eici,
                false,
            ));
            if !b.tasa_if.is_zero() {
                lineas.push(importe_fijo(
                    "Tasa de registro de instalación frigorífica (repercutida)".into(),
                    b.tasa_if,
                    false,
                ));
            }
        }
        c.partidas.push(PartidaLeg {
            sufijo: "RSIF",
            resumen: "Justificación del Reglamento de Seguridad para Instalaciones Frigoríficas (RSIF)".into(),
            texto: "Justificación de la carga de refrigerante por circuito y local (RSIF e IF), incluido el \
                    registro de la instalación frigorífica cuando proceda."
                .into(),
            lineas,
        });
    }

    // 4. Desplazamiento
    if e.km > b.km_umbral && e.viajes > 0 {
        let v = d(e.viajes);
        let mut lineas = vec![por_unidad(
            format!(
                "Kilometraje ({} km ida y vuelta × {} {})",
                corto(e.km),
                e.viajes,
                if e.viajes == 1 { "viaje" } else { "viajes" }
            ),
            "km",
            e.km * v,
            b.km,
            true,
        )];
        if e.horas_viaje > Decimal::ZERO {
            lineas.push(Linea {
                resumen: "Horas de viaje del técnico".into(),
                unidad: "h".into(),
                naturaleza: Naturaleza::ManoObra,
                cantidad: e.horas_viaje * v,
                precio: b.hora_viaje,
                propia: true,
            });
        }
        c.partidas.push(PartidaLeg {
            sufijo: "DESP",
            resumen: format!("Desplazamientos fuera de zona ({sigla})"),
            texto: "Desplazamientos del técnico a obra e inspecciones fuera de la zona habitual.".into(),
            lineas,
        });
    }

    // 5. Repercutidos
    if e.repercutir {
        let lineas = repercutidos(b, e, &mut c.avisos);
        if !lineas.is_empty() {
            c.partidas.push(PartidaLeg {
                sufijo: "REP",
                resumen: format!("Tasas, inspección y visado repercutidos ({sigla})"),
                texto: format!(
                    "Tasas de la Comunidad Autónoma, tarifa de la entidad de inspección (EICI) y, en su caso, \
                     derechos de visado de las {}, repercutidos sin IVA.",
                    e.disciplina.nombre()
                ),
                lineas,
            });
        }
    }

    // Totales y horas
    for p in &c.partidas {
        for l in &p.lineas {
            if l.propia {
                c.honorarios += l.importe();
            } else {
                c.repercutidos += l.importe();
            }
        }
    }
    if b.hora_referencia > Decimal::ZERO {
        c.horas = Some(redondear(c.honorarios / b.hora_referencia, 1));
    }
    if e.pem <= Decimal::ZERO {
        c.avisos
            .push("El PEM de la disciplina es cero: la tramitación queda en el mínimo.".into());
    }
    c
}

fn repercutidos(b: &Baremo, e: &Expediente, avisos: &mut Vec<String>) -> Vec<Linea> {
    let mut r = Vec::new();
    let proyecto = e.documento == Documento::Proyecto;
    let viviendas = e.n_viviendas();
    let n_viv = d(viviendas);
    let fijo = |r: &mut Vec<Linea>, txt: String, v: Decimal| {
        if !v.is_zero() {
            r.push(importe_fijo(txt, v, false));
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
                        false,
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
                        false,
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

/// Código del subcapítulo de una disciplina.
pub fn subcapitulo(disciplina: Disciplina) -> String {
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

/// Escribe (o sustituye) el subcapítulo `LEG-<disciplina>` dentro de `LEG`.
/// Las partidas llevan su descompuesto; si algo falla, el presupuesto queda
/// como estaba.
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
    let sub = subcapitulo(disciplina);
    if p.conceptos.contains_key(&sub) {
        p.quitar_y_purgar(CAPITULO, &sub)?;
    }
    p.nuevo_capitulo(CAPITULO, &sub, &format!("Legalización {}", disciplina.nombre()))?;
    for partida in &calculo.partidas {
        let codigo = format!("{sub}-{}", partida.sufijo);
        let mut lineas = Vec::new();
        for (i, l) in partida.lineas.iter().enumerate() {
            let rc = format!("{codigo}-{:02}", i + 1);
            p.insertar(Concepto::basico(&rc, &l.unidad, &l.resumen, l.naturaleza, l.precio))?;
            lineas.push(LineaDescomposicion::new(rc, l.cantidad));
        }
        let mut c = Concepto::partida(&codigo, "ud", &partida.resumen, lineas);
        c.texto = Some(partida.texto.clone());
        p.insertar(c)?;
        p.anadir_linea(&sub, &codigo, Decimal::ONE)?;
    }
    p.validar()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn b() -> Baremo {
        Baremo::ejemplo(2026)
    }

    #[test]
    fn tramos_acumulativos() {
        // 1 % hasta 100.000 y 0,5 % del exceso: 150.000 → 1.000 + 250.
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(150000));
        let c = calcular(&b(), &e);
        let tram = &c.partidas[0];
        assert_eq!(tram.sufijo, "TRAM");
        assert_eq!(tram.lineas.len(), 2);
        assert_eq!(tram.importe(), dec!(1250));
    }

    #[test]
    fn minimo_por_expediente_y_por_vivienda() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(20000));
        // Mínimo de memoria 300; en proyecto, 500.
        assert_eq!(calcular(&b(), &e).partidas[0].importe(), dec!(300));
        e.documento = Documento::Proyecto;
        assert_eq!(calcular(&b(), &e).partidas[0].importe(), dec!(500));
        e.documento = Documento::Memoria;
        // 40 viviendas × 50 = 2.000 > 1 % de 80.000 = 800.
        e.pem = dec!(80000);
        e.viviendas = 40;
        // En terciario el nº de viviendas no cuenta.
        assert_eq!(calcular(&b(), &e).partidas[0].importe(), dec!(800));
        e.uso = Uso::Viviendas;
        let c = calcular(&b(), &e);
        assert_eq!(c.partidas[0].lineas[0].cantidad, dec!(40));
        assert_eq!(c.partidas[0].importe(), dec!(2000));
        // Repercutidos por vivienda: tasa 3 + EICI 60.
        let rep = c.partidas.iter().find(|p| p.sufijo == "REP").unwrap();
        assert_eq!(rep.importe(), dec!(40) * dec!(63));
    }

    #[test]
    fn recargo_proyecto_ajeno() {
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(10000));
        e.ajeno = true;
        // mínimo 500 + 20 %, y sin redacción.
        let c = calcular(&b(), &e);
        assert_eq!(c.partidas[0].importe(), dec!(600));
        assert!(!c.partidas.iter().any(|p| p.sufijo == "RED"));
        // Proyecto propio: redacción 1,5 % de 10.000 = 150 < mínimo 600.
        e.ajeno = false;
        let c = calcular(&b(), &e);
        assert_eq!(c.partidas[0].importe(), dec!(500));
        assert_eq!(
            c.partidas.iter().find(|p| p.sufijo == "RED").unwrap().importe(),
            dec!(600)
        );
    }

    #[test]
    fn proyecto_lleva_oca_y_visado_con_direccion_de_obra() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(200000));
        e.potencia_kw = dec!(250);
        let c = calcular(&b(), &e);
        assert!(c.partidas.iter().any(|p| p.sufijo == "OCA"));
        let rep = c.partidas.iter().find(|p| p.sufijo == "REP").unwrap();
        // Visado: 100.000 × 0,002 + 100.000 × 0,001 = 300, dos veces (proyecto y D.O.).
        let visados: Vec<_> = rep.lineas.iter().filter(|l| l.resumen.contains("visado")).collect();
        assert_eq!(visados.len(), 2);
        assert!(visados.iter().all(|l| l.importe() == dec!(300)));
        // Tasa 50 + EICI 800 (hasta 1.000 kW).
        assert_eq!(rep.importe(), dec!(50) + dec!(800) + dec!(600));
        // Las memorias no llevan visado ni visita de OCA.
        e.documento = Documento::Memoria;
        let c = calcular(&b(), &e);
        assert!(!c.partidas.iter().any(|p| p.sufijo == "OCA"));
        assert!(
            !c.partidas
                .iter()
                .flat_map(|p| &p.lineas)
                .any(|l| l.resumen.contains("visado"))
        );
    }

    #[test]
    fn eici_por_encima_del_ultimo_tramo_de_potencia() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(50000));
        e.potencia_kw = dec!(1250); // 800 + 10 × ceil(250/100) = 830
        let c = calcular(&b(), &e);
        let rep = c.partidas.iter().find(|p| p.sufijo == "REP").unwrap();
        assert!(
            rep.lineas
                .iter()
                .any(|l| l.resumen.starts_with("EICI") && l.importe() == dec!(830))
        );
    }

    #[test]
    fn justificaciones_rsif_y_desplazamiento() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(50000));
        e.justificar_rite = true;
        e.circuitos_rsif = 3;
        e.registro_if = Some(RegistroIf::MemoriaA2l);
        e.km = dec!(120);
        e.horas_viaje = dec!(2);
        e.viajes = 2;
        let c = calcular(&b(), &e);
        let j = c.partidas.iter().find(|p| p.sufijo == "JRITE").unwrap();
        assert_eq!(j.importe(), dec!(200)); // 0,2 % de 50.000 = 100 < mínimo 200
        let rsif = c.partidas.iter().find(|p| p.sufijo == "RSIF").unwrap();
        assert_eq!(rsif.importe(), dec!(300) + dec!(120) + dec!(35));
        // 40 circuitos de 2 modelos: 2 × 100 + 38 × 20.
        e.circuitos_rsif = 40;
        e.modelos_rsif = 2;
        let c = calcular(&b(), &e);
        let rsif = c.partidas.iter().find(|p| p.sufijo == "RSIF").unwrap();
        assert_eq!(rsif.importe(), dec!(200) + dec!(760) + dec!(120) + dec!(35));
        let desp = c.partidas.iter().find(|p| p.sufijo == "DESP").unwrap();
        assert_eq!(desp.importe(), dec!(240) * dec!(0.25) + dec!(4) * dec!(40));
        // Por debajo del umbral no hay desplazamiento.
        e.km = dec!(40);
        assert!(!calcular(&b(), &e).partidas.iter().any(|p| p.sufijo == "DESP"));
    }

    #[test]
    fn pci_proyecto_por_instalaciones_y_superficie() {
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(150000));
        e.instalaciones_pci = 5; // se queda en la última fila
        e.superficie = dec!(2000); // segunda columna
        e.presion_diferencial = true;
        let c = calcular(&b(), &e);
        let rep = c.partidas.iter().find(|p| p.sufijo == "REP").unwrap();
        // Tasa: 10 × ceil(150.000 / 5.000) = 300; EICI 550; presión 150; visado 200+50 dos veces.
        assert_eq!(rep.importe(), dec!(300) + dec!(550) + dec!(150) + dec!(500));
        assert!(!c.partidas.iter().any(|p| p.sufijo == "JRITE"));
    }

    #[test]
    fn horas_equivalentes_solo_cuentan_honorarios() {
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(100000));
        let c = calcular(&b(), &e);
        // Tramitación 1.000 + redacción 1.500 + OCA 100 (justificación RITE incluida).
        assert_eq!(c.honorarios, dec!(1000) + dec!(1500) + dec!(100));
        assert_eq!(c.horas, Some(dec!(65)));
        assert!(c.repercutidos > Decimal::ZERO);
    }

    #[test]
    fn aplicar_crea_y_sustituye_el_subcapitulo() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        let pem_antes = p.pem().unwrap();
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, pem_antes);
        let c = calcular(&b(), &e);
        aplicar(&mut p, Disciplina::Rite, &c).unwrap();
        assert_eq!(p.pem().unwrap(), pem_antes + c.total());
        // Repetir no duplica.
        aplicar(&mut p, Disciplina::Rite, &c).unwrap();
        assert_eq!(p.pem().unwrap(), pem_antes + c.total());
        assert_eq!(p.concepto(CAPITULO).unwrap().descomposicion.len(), 1);
        // La base del PEM no incluye la legalización.
        let suma: Decimal = capitulos(&p).unwrap().iter().map(|x| x.2).sum();
        assert_eq!(suma, pem_antes);
        let tram = p.concepto("LEG-RITE-TRAM").unwrap();
        assert!(!tram.descomposicion.is_empty());
        assert!(tram.texto.as_deref().unwrap().contains("as-built"));
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
        // Campos que faltan toman su valor por defecto.
        let parcial: Baremos = serde_json::from_str(r#"{"versiones":[{"anio":2026,"km":"0.26"}]}"#).unwrap();
        assert_eq!(parcial.versiones[0].km, dec!(0.26));
        assert!(parcial.versiones[0].visado_direccion_obra);
    }
}
