// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-legalizacion
//!
//! Legalización **as-built** (sobre la instalación ya ejecutada) de una disciplina de la obra (RITE o PCI, con RSIF) como
//! **una partida descompuesta** en el capítulo `LEG`, en la que figuran todos
//! los costes:
//!
//! 1. **Mano de obra técnica** por perfil (senior, junior, técnico,
//!    documentación) a su coste horario: horas de cada tarea (fijas, por
//!    10.000 € de PEM y por vivienda) para la memoria o la tramitación y
//!    dirección de la instalación, la redacción del proyecto propio y las
//!    justificaciones RITE y RSIF, y horas de las visitas a obra.
//! 2. **Visitas de la OCA**: horas del técnico en la inspección.
//! 3. **Desplazamientos**: horas de viaje, kilometraje, aparcamiento y dietas
//!    de cada visita (dirección de obra y OCA).
//! 4. **Costes indirectos, gestión y beneficio**: % sobre lo anterior, según
//!    el tamaño del encargo (pequeño, mediano, grande).
//! 5. **Repercutidos** sin margen: tasas de la Comunidad, EICI y visados.
//!
//! La tarifa sale de nuestros costes y nuestro margen; los honorarios en % del
//! PEM se calculan solo como **referencia de mercado** para comparar.
//!
//! El baremo (horas, costes horarios, margen, referencias y tablas) es del usuario y vive
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

/// Horas de una tarea: fijas + por cada 10.000 € de PEM + por vivienda,
/// con un mínimo.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Horas {
    pub base: Decimal,
    pub por_10k: Decimal,
    pub por_vivienda: Decimal,
    pub minimo: Decimal,
}

impl Horas {
    fn de(&self, pem: Decimal, viviendas: Decimal) -> Decimal {
        (self.base + self.por_10k * pem / d(10_000) + self.por_vivienda * viviendas).max(self.minimo)
    }

    fn texto(&self, viviendas: Decimal) -> String {
        let mut t = format!("{} h", corto(self.base));
        if !self.por_10k.is_zero() {
            t.push_str(&format!(" + {} h por 10.000 € de PEM", corto(self.por_10k)));
        }
        if !self.por_vivienda.is_zero() && !viviendas.is_zero() {
            t.push_str(&format!(" + {} h por vivienda", corto(self.por_vivienda)));
        }
        if !self.minimo.is_zero() {
            t.push_str(&format!("; mínimo {} h", corto(self.minimo)));
        }
        t
    }
}

/// Tamaño del encargo: decide el margen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tamano {
    Pequeno,
    Mediano,
    Grande,
}

impl Tamano {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Pequeno => "pequeño",
            Self::Mediano => "mediano",
            Self::Grande => "grande",
        }
    }
}

/// Costes, horas, margen y tablas de un año. Todos los importes sin IVA.
///
/// La tarifa se forma con **nuestros costes**: horas de cada tarea por su
/// coste horario, desplazamientos y el margen. Los honorarios en % del PEM
/// son solo la **referencia de mercado** con la que se compara.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Baremo {
    pub anio: i32,
    /// Procedencia de los datos (p. ej. «Tablas PRMEM 2026, honorarios AIDESEGI»).
    pub nota: String,

    // --- referencia de mercado (honorarios en % del PEM; no forman el precio)
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

    // --- horas por tarea (forman el precio)
    pub horas_memoria: Horas,
    pub horas_tramitacion: Horas,
    /// Redacción del proyecto: memoria, presupuesto, pliego de condiciones,
    /// seguridad y salud y anexos (certificados, fichas, manuales de usuario).
    pub horas_redaccion: Horas,
    /// Redacción del proyecto PCI (menos documento); sin datos, la de RITE.
    pub horas_redaccion_pci: Horas,
    /// Proyectos: planos as-built y cálculos justificativos, aparte.
    pub horas_planos: Horas,
    /// Cada revisión de los planos de arquitectura añade este % de las horas
    /// de planos as-built.
    pub pct_revision_planos: Decimal,
    pub horas_calculos: Horas,
    pub horas_justif_rite: Horas,
    /// RSIF: horas fijas + por modelo + por circuito repetido.
    pub rsif_horas_base: Decimal,
    pub rsif_horas_modelo: Decimal,
    pub rsif_horas_repetido: Decimal,
    /// Horas del técnico en cada visita a obra.
    pub horas_visita_obra: Decimal,
    /// Horas de la jornada: el total de horas se redondea al alza a jornadas
    /// completas (0 = sin redondeo).
    pub horas_jornada: Decimal,
    /// Margen (costes indirectos, gestión y beneficio) según tamaño del
    /// encargo: `margen` para pequeños; 0 en mediano o grande = el de pequeño.
    pub margen_mediano: Decimal,
    pub margen_grande: Decimal,
    /// Horas a partir de las que el encargo es mediano o grande.
    pub horas_mediano: Decimal,
    pub horas_grande: Decimal,

    // --- costes
    /// Coste horario de cada perfil (€/h).
    pub coste_senior: Decimal,
    pub coste_junior: Decimal,
    pub coste_tecnico: Decimal,
    pub coste_documentacion: Decimal,
    /// Costes indirectos, gestión y beneficio (%) de los encargos pequeños.
    pub margen: Decimal,
    /// Reparto de horas de la tramitación de proyectos y de memorias.
    pub reparto_tramitacion: Reparto,
    pub reparto_memoria: Reparto,
    pub reparto_redaccion: Reparto,
    pub reparto_planos: Reparto,
    pub reparto_calculos: Reparto,
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
            horas_memoria: Horas::default(),
            horas_tramitacion: Horas::default(),
            horas_redaccion: Horas::default(),
            horas_redaccion_pci: Horas::default(),
            horas_planos: Horas::default(),
            pct_revision_planos: z,
            horas_calculos: Horas::default(),
            rsif_horas_base: z,
            horas_justif_rite: Horas::default(),
            rsif_horas_modelo: z,
            rsif_horas_repetido: z,
            horas_visita_obra: z,
            horas_jornada: z,
            margen_mediano: z,
            margen_grande: z,
            horas_mediano: z,
            horas_grande: z,
            reparto_memoria: reparto_memoria(),
            visitas_memoria: 1,
            visitas_proyecto: 2,
            tramitacion_memoria: Vec::new(),
            reparto_redaccion: reparto_redaccion(),
            reparto_planos: Reparto {
                senior: d(5),
                junior: d(25),
                tecnico: Decimal::ZERO,
                documentacion: d(70),
            },
            reparto_calculos: Reparto {
                senior: d(30),
                junior: d(70),
                tecnico: Decimal::ZERO,
                documentacion: Decimal::ZERO,
            },
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
            margen_mediano: d(30),
            margen_grande: d(35),
            horas_mediano: d(20),
            horas_grande: d(80),
            horas_memoria: Horas {
                base: d(4),
                por_10k: Decimal::new(4, 1),
                por_vivienda: d(1),
                minimo: Decimal::ZERO,
            },
            horas_tramitacion: Horas {
                base: d(8),
                por_10k: d(1),
                por_vivienda: Decimal::new(5, 1),
                minimo: Decimal::ZERO,
            },
            horas_redaccion: Horas {
                base: d(12),
                por_10k: Decimal::new(15, 1),
                por_vivienda: Decimal::ZERO,
                minimo: Decimal::ZERO,
            },
            horas_justif_rite: Horas {
                base: d(2),
                por_10k: Decimal::new(1, 1),
                por_vivienda: Decimal::ZERO,
                minimo: Decimal::ZERO,
            },
            horas_planos: Horas {
                base: d(6),
                por_10k: Decimal::new(25, 2),
                por_vivienda: Decimal::ZERO,
                minimo: Decimal::ZERO,
            },
            horas_calculos: Horas {
                base: d(4),
                por_10k: Decimal::new(2, 1),
                por_vivienda: Decimal::ZERO,
                minimo: Decimal::ZERO,
            },
            pct_revision_planos: d(40),
            rsif_horas_base: d(1),
            rsif_horas_modelo: d(1),
            rsif_horas_repetido: Decimal::new(25, 2),
            horas_visita_obra: d(2),
            horas_jornada: d(8),
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

    /// Margen del tamaño de encargo.
    pub fn margen_de(&self, t: Tamano) -> Decimal {
        let m = match t {
            Tamano::Pequeno => self.margen,
            Tamano::Mediano => self.margen_mediano,
            Tamano::Grande => self.margen_grande,
        };
        if m.is_zero() { self.margen } else { m }
    }

    /// Tamaño según las horas de mano de obra.
    pub fn tamano_por_horas(&self, horas: Decimal) -> Tamano {
        if !self.horas_grande.is_zero() && horas >= self.horas_grande {
            Tamano::Grande
        } else if !self.horas_mediano.is_zero() && horas >= self.horas_mediano {
            Tamano::Mediano
        } else {
            Tamano::Pequeno
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
    /// RITE: suma de las potencias térmicas nominales de los generadores de
    /// calor y, por separado, de los de frío del edificio (RITE art. 15.2).
    pub potencia_calor_kw: Decimal,
    pub potencia_frio_kw: Decimal,
    /// RITE: edificio de viviendas con sala de máquinas (cambia la EICI).
    pub sala_maquinas: bool,
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
    /// Circuitos frigoríficos a justificar por RSIF (0 = sin RSIF: no se
    /// supera la carga mínima) y, de ellos, modelos distintos (0 = todos distintos).
    pub circuitos_rsif: u32,
    pub modelos_rsif: u32,
    pub registro_if: Option<RegistroIf>,
    /// Revisiones de los planos de arquitectura que obligan a rehacer los
    /// planos as-built.
    pub revisiones_arquitectura: u32,
    /// Visitas de dirección de obra y segundas visitas de la OCA.
    pub visitas_obra: u32,
    pub segundas_visitas: u32,
    /// Km de ida y vuelta y horas por viaje (0 = viaje habitual del baremo).
    pub km: Decimal,
    pub horas_viaje: Decimal,
    /// Repercutir tasas, EICI y visado (si no, los paga la propiedad).
    pub repercutir: bool,
    /// Tamaño del encargo para el margen (`None` = según las horas).
    pub tamano: Option<Tamano>,
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
            potencia_calor_kw: Decimal::ZERO,
            potencia_frio_kw: Decimal::ZERO,
            sala_maquinas: false,
            primario_frio: false,
            instalaciones_pci: 1,
            superficie: Decimal::ZERO,
            presion_diferencial: false,
            justificar_rite: disciplina == Disciplina::Rite && documento == Documento::Memoria,
            circuitos_rsif: 0,
            modelos_rsif: 0,
            registro_if: None,
            revisiones_arquitectura: 0,
            visitas_obra: if documento == Documento::Proyecto { 2 } else { 1 },
            segundas_visitas: 0,
            km: Decimal::ZERO,
            horas_viaje: Decimal::ZERO,
            repercutir: true,
            tamano: None,
        }
    }

    /// Potencia que decide la documentación RITE: la mayor de la suma de
    /// generadores de calor y la de frío (art. 15.2).
    pub fn potencia(&self) -> Decimal {
        self.potencia_calor_kw.max(self.potencia_frio_kw)
    }

    /// Documento que exige el RITE (art. 15): proyecto por encima de 70 kW,
    /// memoria técnica de 5 a 70 kW y ninguno por debajo de 5 kW. `None` si no
    /// es RITE o no se conoce la potencia.
    pub fn documento_exigido(&self) -> Option<Option<Documento>> {
        let p = self.potencia();
        if self.disciplina != Disciplina::Rite || p.is_zero() {
            None
        } else if p > d(70) {
            Some(Some(Documento::Proyecto))
        } else if p >= d(5) {
            Some(Some(Documento::Memoria))
        } else {
            Some(None)
        }
    }

    /// Circuitos RSIF de modelos distintos y repetidos.
    pub fn rsif_modelos_repetidos(&self) -> (u32, u32) {
        let m = if self.modelos_rsif == 0 {
            self.circuitos_rsif
        } else {
            self.modelos_rsif.min(self.circuitos_rsif)
        };
        (m, self.circuitos_rsif - m)
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
    /// Tamaño aplicado y su margen (%).
    pub tamano: Option<Tamano>,
    pub pct_margen: Decimal,
    /// Honorarios de referencia de mercado (baremo en % del PEM), para comparar.
    pub referencia: Decimal,
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

/// Líneas de mano de obra: `horas` repartidas por perfil a su coste.
fn horas_perfil(b: &Baremo, que: &str, horas: Decimal, r: &Reparto) -> Vec<Linea> {
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

/// Honorarios de referencia de mercado (baremo en % del PEM con mínimos).
pub fn referencia(b: &Baremo, e: &Expediente) -> Decimal {
    let proyecto = e.proyecto();
    let n_viv = d(e.n_viviendas());
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
    let mut r = por_pem(tramos, e.pem).0.max(minimo).max(b.minimo_vivienda * n_viv);
    if e.ajeno {
        r = redondear(r * (CIEN + b.recargo_ajeno) / CIEN, 2);
    }
    if proyecto && !e.ajeno && !b.redaccion.is_empty() {
        r += por_pem(&b.redaccion, e.pem).0.max(b.redaccion_minimo);
    }
    if e.disciplina == Disciplina::Rite && e.justificar_rite {
        r += redondear(e.pem * b.justif_rite_pct / CIEN, 2).max(b.justif_rite_minimo);
    }
    if e.circuitos_rsif > 0 {
        let (m, rep) = e.rsif_modelos_repetidos();
        let p_rep = if b.rsif_repetido.is_zero() {
            b.rsif_circuito
        } else {
            b.rsif_repetido
        };
        r += (d(m) * b.rsif_circuito + d(rep) * p_rep).max(b.rsif_minimo);
    }
    r
}

/// Calcula la partida de legalización de un expediente con un baremo:
/// nuestras horas a nuestro coste, desplazamientos y margen, y los
/// repercutidos sin margen.
pub fn calcular(b: &Baremo, e: &Expediente) -> Calculo {
    let mut c = Calculo::default();
    let proyecto = e.proyecto();
    let doc = if proyecto { "proyecto" } else { "memoria técnica" };
    let viviendas = e.n_viviendas();
    let n_viv = d(viviendas);

    // 1. Memoria técnica, o tramitación y dirección de la instalación
    let (titulo, que, horas, reparto) = if proyecto {
        (
            "Tramitación y dirección de la instalación",
            "tramitación y dirección de la instalación",
            &b.horas_tramitacion,
            &b.reparto_tramitacion,
        )
    } else {
        (
            "Elaboración y tramitación de la memoria técnica",
            "memoria técnica, as-built y tramitación",
            &b.horas_memoria,
            &b.reparto_memoria,
        )
    };
    let mut h = horas.de(e.pem, n_viv);
    let mut regla = horas.texto(n_viv);
    if e.ajeno && !b.recargo_ajeno.is_zero() {
        h = h * (CIEN + b.recargo_ajeno) / CIEN;
        regla.push_str(&format!(
            ", +{} % por revisar el proyecto de otro técnico",
            corto(b.recargo_ajeno)
        ));
    }
    c.bloques.push(Bloque {
        titulo: titulo.into(),
        regla: format!("{} h ({regla})", num(h, 1)),
        lineas: horas_perfil(b, que, h, reparto),
    });

    // 2. Proyecto: redacción (si es propio), planos as-built y cálculos
    if proyecto {
        let mut tareas = Vec::new();
        if !e.ajeno {
            tareas.push((
                "Redacción del proyecto de legalización (as-built)",
                "redacción: memoria, presupuesto, pliego, seguridad y salud y anexos",
                if e.disciplina == Disciplina::Pci && b.horas_redaccion_pci != Horas::default() {
                    &b.horas_redaccion_pci
                } else {
                    &b.horas_redaccion
                },
                &b.reparto_redaccion,
            ));
        }
        let revisiones = e.revisiones_arquitectura;
        tareas.push(("Planos as-built", "planos as-built", &b.horas_planos, &b.reparto_planos));
        if !e.ajeno {
            tareas.push((
                "Cálculos justificativos",
                "cálculos justificativos",
                &b.horas_calculos,
                &b.reparto_calculos,
            ));
        }
        for (titulo, que, horas, reparto) in tareas {
            let mut h = horas.de(e.pem, n_viv);
            let mut regla = horas.texto(n_viv);
            if titulo == "Planos as-built" && revisiones > 0 && !b.pct_revision_planos.is_zero() {
                h = h * (CIEN + d(revisiones) * b.pct_revision_planos) / CIEN;
                regla.push_str(&format!(
                    "; + {revisiones} revisiones de arquitectura × {} %",
                    corto(b.pct_revision_planos)
                ));
            }
            if h > Decimal::ZERO {
                c.bloques.push(Bloque {
                    titulo: titulo.into(),
                    regla: format!("{} h ({regla})", num(h, 1)),
                    lineas: horas_perfil(b, que, h, reparto),
                });
            }
        }
    }

    // 3. Justificaciones
    if e.disciplina == Disciplina::Rite && e.justificar_rite {
        let h = b.horas_justif_rite.de(e.pem, n_viv);
        if h > Decimal::ZERO {
            c.bloques.push(Bloque {
                titulo: "Justificación RITE".into(),
                regla: format!("{} h ({})", num(h, 1), b.horas_justif_rite.texto(n_viv)),
                lineas: horas_perfil(b, "justificación RITE", h, &b.reparto_justificacion),
            });
        }
    }
    let mut rep_if = Vec::new();
    if e.circuitos_rsif > 0 {
        let (modelos, repetidos) = e.rsif_modelos_repetidos();
        let h = b.rsif_horas_base + d(modelos) * b.rsif_horas_modelo + d(repetidos) * b.rsif_horas_repetido;
        if h > Decimal::ZERO {
            c.bloques.push(Bloque {
                titulo: format!("Justificación RSIF ({} circuitos)", e.circuitos_rsif),
                regla: format!(
                    "{} h ({} h + {modelos} modelos × {} h + {repetidos} repetidos × {} h)",
                    num(h, 1),
                    corto(b.rsif_horas_base),
                    corto(b.rsif_horas_modelo),
                    corto(b.rsif_horas_repetido)
                ),
                lineas: horas_perfil(b, "justificación RSIF", h, &b.reparto_justificacion),
            });
        }
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

    // 4. Visitas: de obra y de la OCA
    if e.visitas_obra > 0 && !b.horas_visita_obra.is_zero() {
        c.bloques.push(Bloque {
            titulo: format!("Visitas a obra ({})", e.visitas_obra),
            regla: format!("{} h por visita", corto(b.horas_visita_obra)),
            lineas: vec![por_unidad(
                format!(
                    "{}: {} en obra",
                    Perfil::Tecnico.nombre(),
                    if proyecto {
                        "dirección de la instalación"
                    } else {
                        "comprobación"
                    }
                ),
                "h",
                b.horas_visita_obra * d(e.visitas_obra),
                b.coste_tecnico,
                Tipo::ManoObra,
            )],
        });
    }
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
                    "{} visitas a obra + {visitas_oca} de la OCA; {} km y {} h por viaje",
                    e.visitas_obra,
                    corto(km),
                    corto(horas)
                ),
                lineas,
            });
        }
    }

    // 5b. Jornadas completas: el total de horas se redondea al alza a jornadas
    // y la diferencia se añade al primer bloque, repartida como él.
    let mut horas_mo: Decimal = c.lineas().filter(|l| l.unidad == "h").map(|l| l.cantidad).sum();
    if b.horas_jornada > Decimal::ZERO && horas_mo > Decimal::ZERO {
        let jornadas = (horas_mo / b.horas_jornada).ceil();
        let extra = jornadas * b.horas_jornada - horas_mo;
        if extra > Decimal::ZERO
            && let Some(bl) = c.bloques.first_mut()
        {
            let mut nuevas = horas_perfil(b, "ajuste a jornada completa", extra, reparto);
            // El redondeo de cada línea no debe descuadrar el total.
            let suma: Decimal = nuevas.iter().map(|l| l.cantidad).sum();
            if let Some(ultima) = nuevas.last_mut() {
                ultima.cantidad += extra - suma;
            }
            bl.lineas.extend(nuevas);
            bl.regla.push_str(&format!(
                "; + {} h para completar {} jornadas de {} h",
                num(extra, 1),
                jornadas,
                corto(b.horas_jornada)
            ));
            horas_mo = jornadas * b.horas_jornada;
        }
    }

    // 6. Costes indirectos, gestión y beneficio (según tamaño del encargo)
    let tamano = e.tamano.unwrap_or_else(|| b.tamano_por_horas(horas_mo));
    let pct = b.margen_de(tamano);
    c.tamano = Some(tamano);
    c.pct_margen = pct;
    let base: Decimal = c.lineas().map(Linea::importe).sum();
    if !pct.is_zero() {
        let precio = redondear(base / CIEN, 3);
        c.bloques.push(Bloque {
            titulo: "Costes indirectos, gestión y beneficio".into(),
            regla: format!("{} % (encargo {}) sobre {}", corto(pct), tamano.nombre(), eur(base)),
            lineas: vec![Linea {
                resumen: "Costes indirectos, gestión y beneficio de ingeniería".into(),
                unidad: "%".into(),
                cantidad: pct,
                precio,
                tipo: Tipo::Margen,
            }],
        });
    }
    c.referencia = referencia(b, e);

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
    match e.documento_exigido() {
        Some(Some(Documento::Proyecto)) if !proyecto => c.avisos.push(format!(
            "Con {} kW (suma de generadores, RITE art. 15.2) hace falta proyecto, no memoria técnica.",
            corto(e.potencia())
        )),
        Some(Some(Documento::Memoria)) if proyecto => c.avisos.push(format!(
            "Con {} kW basta con memoria técnica (RITE art. 15); el proyecto es opcional.",
            corto(e.potencia())
        )),
        Some(None) => c.avisos.push(format!(
            "Con {} kW (menos de 5 kW) el RITE no exige memoria ni proyecto.",
            corto(e.potencia())
        )),
        _ => {}
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
            "Legalización de las instalaciones térmicas (RITE) mediante proyecto{}. Incluye memoria, presupuesto, \
             pliego de condiciones, estudio de seguridad y salud y anexos (certificados, fichas y manuales), \
             dirección de la instalación y certificado final, planos as-built, cálculos justificativos, libro de uso y \
             mantenimiento, cumplimentación de la documentación 3.1.5, aportación del CIE del instalador \
             eléctrico, asistencia a la inspección de la OCA, desplazamientos y gestión del expediente ante \
             el órgano competente de la Comunidad Autónoma{tasas}.",
            if e.ajeno {
                " de otro técnico"
            } else {
                ", incluida su redacción sobre la instalación ejecutada (as-built)"
            }
        ),
        (Disciplina::Rite, false) => format!(
            "Legalización de las instalaciones térmicas (RITE) mediante memoria técnica sobre la instalación ejecutada (as-built). Incluye elaboración \
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
                ", incluida su redacción sobre la instalación ejecutada (as-built)"
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
            if viviendas > 0 && !e.sala_maquinas {
                let v = escalon(&b.eici_rite_viviendas, n_viv)
                    .unwrap_or(b.eici_rite_viviendas_base + b.eici_rite_por_vivienda * n_viv);
                fijo(
                    &mut r,
                    format!("EICI proyecto RITE, edificio de {viviendas} viviendas"),
                    v,
                );
            } else {
                let ultimo = b.eici_rite_kw.last().and_then(|tr| tr.hasta);
                let v = match escalon(&b.eici_rite_kw, e.potencia()) {
                    Some(v) => v,
                    None => {
                        let tope = ultimo.unwrap_or(Decimal::ZERO);
                        let base = b.eici_rite_kw.last().map_or(Decimal::ZERO, |tr| tr.valor);
                        base + b.eici_rite_por_100kw * ((e.potencia() - tope) / CIEN).ceil()
                    }
                };
                fijo(&mut r, format!("EICI proyecto RITE ({} kW)", num(e.potencia(), 0)), v);
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

    /// Baremo de ejemplo sin redondeo a jornadas, para comprobar cada tarea.
    fn b() -> Baremo {
        Baremo {
            horas_jornada: Decimal::ZERO,
            ..Baremo::ejemplo(2026)
        }
    }

    fn bloque<'a>(c: &'a Calculo, titulo: &str) -> Option<&'a Bloque> {
        c.bloques.iter().find(|b| b.titulo.starts_with(titulo))
    }

    fn horas(c: &Calculo, titulo: &str) -> Decimal {
        bloque(c, titulo).unwrap().lineas.iter().map(|l| l.cantidad).sum()
    }

    #[test]
    fn horas_por_tarea_a_coste_por_perfil() {
        // Memoria: 4 h + 0,4 h por 10.000 € → 150.000 € = 10 h.
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(150000));
        e.justificar_rite = false;
        e.circuitos_rsif = 0;
        e.visitas_obra = 0;
        let c = calcular(&b(), &e);
        let m = bloque(&c, "Elaboración").unwrap();
        assert_eq!(horas(&c, "Elaboración"), dec!(10));
        // Reparto de memorias: 10 % senior a 50 €/h, 50 % junior a 40…
        assert_eq!(m.lineas[0].cantidad, dec!(1));
        assert_eq!(m.lineas[0].precio, dec!(50));
        // Coste = Σ horas × coste; margen de encargo pequeño (25 %).
        assert_eq!(
            c.coste,
            dec!(1) * dec!(50) + dec!(5) * dec!(40) + dec!(1) * dec!(40) + dec!(3) * dec!(30)
        );
        assert_eq!(c.tamano, Some(Tamano::Pequeno));
        assert_eq!(c.margen, redondear(redondear(c.coste / CIEN, 3) * dec!(25), 2));
        // El baremo en % del PEM solo es referencia: 1,5 % de 50.000 + 0,75 % de 100.000.
        assert_eq!(c.referencia, dec!(1500));
    }

    #[test]
    fn horas_con_minimo() {
        let h = Horas {
            base: d(24),
            por_10k: Decimal::new(2, 1),
            por_vivienda: Decimal::ZERO,
            minimo: d(32),
        };
        assert_eq!(h.de(dec!(120000), Decimal::ZERO), dec!(32));
        assert_eq!(h.de(dec!(400000), Decimal::ZERO), dec!(32));
        assert_eq!(h.de(dec!(1000000), Decimal::ZERO), dec!(44));
        assert!(h.texto(Decimal::ZERO).contains("mínimo 32 h"));
    }

    #[test]
    fn redaccion_pci_propia() {
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(200000));
        // Sin horas de PCI se usan las de RITE: 12 + 1,5 × 20.
        assert_eq!(horas(&calcular(&b(), &e), "Redacción"), dec!(42));
        let mut bp = b();
        bp.horas_redaccion_pci = Horas {
            base: d(10),
            por_10k: Decimal::ZERO,
            por_vivienda: Decimal::ZERO,
            minimo: d(20),
        };
        assert_eq!(horas(&calcular(&bp, &e), "Redacción"), dec!(20));
        e.disciplina = Disciplina::Rite;
        assert_eq!(horas(&calcular(&bp, &e), "Redacción"), dec!(42));
    }

    #[test]
    fn horas_en_jornadas_completas() {
        // Memoria de 7.000 €: 4,28 h + 2,07 + 1,5 + visita 2 + viaje 1 = 10,85 → 16 h.
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(7000));
        let c = calcular(&Baremo::ejemplo(2026), &e);
        assert_eq!(c.horas, dec!(16));
        let m = bloque(&c, "Elaboración").unwrap();
        assert!(m.regla.contains("2 jornadas de 8 h"));
        assert!(m.lineas.iter().any(|l| l.resumen.contains("ajuste a jornada completa")));
        // Sin jornada no se redondea.
        assert!(calcular(&b(), &e).horas < dec!(11));
    }

    #[test]
    fn proyecto_tramitacion_redaccion_y_ajeno() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(200000));
        e.potencia_calor_kw = dec!(250);
        let c = calcular(&b(), &e);
        assert_eq!(horas(&c, "Tramitación"), dec!(28)); // 8 + 1 × 20
        assert_eq!(horas(&c, "Redacción del proyecto de legalización"), dec!(42)); // 12 + 1,5 × 20
        assert_eq!(horas(&c, "Planos as-built"), dec!(11)); // 6 + 0,25 × 20
        // 2 revisiones de arquitectura × 40 %: 11 × 1,8.
        let mut r = e.clone();
        r.revisiones_arquitectura = 2;
        assert_eq!(horas(&calcular(&b(), &r), "Planos as-built"), dec!(19.8));
        assert_eq!(horas(&c, "Cálculos"), dec!(8)); // 4 + 0,2 × 20
        assert!(bloque(&c, "Justificación RITE").is_none());
        e.ajeno = true;
        e.justificar_rite = e.justificar_rite_por_defecto();
        let c = calcular(&b(), &e);
        assert!(bloque(&c, "Redacción").is_none());
        assert!(bloque(&c, "Cálculos").is_none());
        assert!(bloque(&c, "Planos as-built").is_some());
        assert_eq!(horas(&c, "Tramitación"), dec!(33.6)); // 28 + 20 %
        assert_eq!(horas(&c, "Justificación RITE"), dec!(4)); // 2 + 0,1 × 20
    }

    #[test]
    fn viviendas_suman_horas_por_vivienda() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(50000));
        e.viviendas = 10;
        let terciario = horas(&calcular(&b(), &e), "Elaboración");
        e.uso = Uso::Viviendas;
        let c = calcular(&b(), &e);
        assert_eq!(horas(&c, "Elaboración"), terciario + dec!(10));
        // Repercutidos por vivienda: tasa 3 + EICI 60, sin margen.
        assert_eq!(c.repercutidos, dec!(10) * dec!(63));
    }

    #[test]
    fn margen_segun_tamano_del_encargo() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(400000));
        e.potencia_calor_kw = dec!(300);
        let c = calcular(&b(), &e);
        assert!(c.horas >= dec!(80));
        assert_eq!(c.tamano, Some(Tamano::Grande));
        assert_eq!(c.pct_margen, dec!(35));
        e.tamano = Some(Tamano::Mediano);
        assert_eq!(calcular(&b(), &e).pct_margen, dec!(30));
        // Sin márgenes por tamaño se usa el general.
        let mut b0 = b();
        b0.margen_grande = Decimal::ZERO;
        e.tamano = None;
        assert_eq!(calcular(&b0, &e).pct_margen, dec!(25));
    }

    #[test]
    fn memoria_rite_lleva_justificacion_rite_y_rsif() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(10000));
        // Sin circuitos: no se supera la carga mínima y no hay RSIF.
        assert!(e.justificar_rite && e.circuitos_rsif == 0);
        let c = calcular(&b(), &e);
        assert_eq!(horas(&c, "Justificación RITE"), dec!(2.1));
        assert!(bloque(&c, "Justificación RSIF").is_none());
        // 3 circuitos distintos: 1 + 3 × 1 = 4 h.
        e.circuitos_rsif = 3;
        assert_eq!(horas(&calcular(&b(), &e), "Justificación RSIF"), dec!(4));
        let pci = Expediente::nuevo(Disciplina::Pci, Documento::Memoria, dec!(7000));
        assert!(!pci.justificar_rite);
        let mut p = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(7000));
        assert!(!p.justificar_rite_por_defecto());
        p.ajeno = true;
        assert!(p.justificar_rite_por_defecto());
    }

    #[test]
    fn rsif_por_modelo_y_repetido_con_registro() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(50000));
        e.circuitos_rsif = 40;
        e.modelos_rsif = 2;
        e.registro_if = Some(RegistroIf::MemoriaA2l);
        let c = calcular(&b(), &e);
        assert_eq!(horas(&c, "Justificación RSIF"), dec!(1) + dec!(2) + dec!(9.5));
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
    fn visitas_y_desplazamientos() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(50000));
        e.potencia_calor_kw = dec!(120);
        e.segundas_visitas = 1;
        let c = calcular(&b(), &e);
        // 2 visitas a obra × 2 h × 40; 2 de la OCA × 2 h × 40.
        assert_eq!(bloque(&c, "Visitas a obra").unwrap().importe(), dec!(160));
        assert_eq!(bloque(&c, "Visitas de inspección").unwrap().importe(), dec!(160));
        // 4 viajes: 4 h × 40 + 160 km × 0,25 + 4 × 10; sin dietas.
        assert_eq!(
            bloque(&c, "Desplazamientos").unwrap().importe(),
            dec!(160) + dec!(40) + dec!(40)
        );
        e.km = dec!(250);
        e.horas_viaje = dec!(3);
        let c = calcular(&b(), &e);
        let desp = bloque(&c, "Desplazamientos").unwrap();
        assert_eq!(
            desp.importe(),
            dec!(12) * dec!(40) + dec!(1000) * dec!(0.25) + dec!(40) + dec!(120)
        );
        // La EICI de segunda visita se repercute sin margen.
        let rep = bloque(&c, "Tasas").unwrap();
        assert!(
            rep.lineas
                .iter()
                .any(|l| l.resumen.contains("segunda visita") && l.importe() == dec!(300))
        );
        let base: Decimal = c
            .lineas()
            .filter(|l| matches!(l.tipo, Tipo::ManoObra | Tipo::Indirecto))
            .map(Linea::importe)
            .sum();
        assert_eq!(c.coste, base);
    }

    #[test]
    fn documentacion_segun_potencia_rite_art_15() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(300000));
        assert_eq!(e.documento_exigido(), None);
        e.uso = Uso::Viviendas;
        e.viviendas = 40;
        e.potencia_calor_kw = dec!(160);
        e.potencia_frio_kw = dec!(180);
        assert_eq!(e.potencia(), dec!(180));
        assert_eq!(e.documento_exigido(), Some(Some(Documento::Proyecto)));
        assert!(
            calcular(&b(), &e)
                .avisos
                .iter()
                .any(|a| a.contains("hace falta proyecto"))
        );
        e.documento = Documento::Proyecto;
        let c = calcular(&b(), &e);
        assert!(c.avisos.is_empty());
        assert!(c.lineas().any(|l| l.resumen.contains("edificio de 40 viviendas")));
        e.sala_maquinas = true;
        assert!(calcular(&b(), &e).lineas().any(|l| l.resumen.contains("(180 kW)")));
        e.potencia_calor_kw = dec!(12);
        e.potencia_frio_kw = dec!(9);
        assert_eq!(e.documento_exigido(), Some(Some(Documento::Memoria)));
        e.potencia_calor_kw = dec!(4);
        e.potencia_frio_kw = dec!(3.5);
        assert_eq!(e.documento_exigido(), Some(None));
    }

    #[test]
    fn repercutidos_de_proyecto() {
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, dec!(200000));
        e.potencia_calor_kw = dec!(250);
        let c = calcular(&b(), &e);
        // Tasa 50 + EICI 800 + visado (200 + 100) × 2.
        assert_eq!(c.repercutidos, dec!(50) + dec!(800) + dec!(600));
        e.potencia_calor_kw = dec!(1250); // 800 + 10 × ceil(250/100)
        let c = calcular(&b(), &e);
        assert!(
            c.lineas()
                .any(|l| l.resumen.starts_with("EICI proyecto") && l.importe() == dec!(830))
        );
        let mut e = Expediente::nuevo(Disciplina::Pci, Documento::Proyecto, dec!(150000));
        e.instalaciones_pci = 5;
        e.superficie = dec!(2000);
        e.presion_diferencial = true;
        assert_eq!(
            calcular(&b(), &e).repercutidos,
            dec!(300) + dec!(550) + dec!(150) + dec!(500)
        );
        // Memoria: sin visado ni OCA.
        let e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, dec!(30000));
        let c = calcular(&b(), &e);
        assert!(bloque(&c, "Visitas de inspección").is_none());
        assert!(!c.lineas().any(|l| l.resumen.contains("visado")));
        assert!(c.texto.contains("comprobación en obra"));
    }

    #[test]
    fn aplicar_crea_una_partida_con_todo_el_descompuesto() {
        let mut p = ppto_core::ejemplos::suelo_radiante();
        let pem_antes = p.pem().unwrap();
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Proyecto, pem_antes);
        e.potencia_calor_kw = dec!(80);
        e.segundas_visitas = 1;
        let c = calcular(&Baremo::ejemplo(2026), &e);
        assert_eq!(c.horas % dec!(8), Decimal::ZERO);
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
        assert_eq!(p.concepto("LEG-RITE%CI").unwrap().naturaleza, Naturaleza::Porcentaje);
        assert!(partida.texto.as_deref().unwrap().contains("as-built"));
        let h = &partida.descomposicion[0].hijo;
        assert_eq!(p.concepto(h).unwrap().naturaleza, Naturaleza::ManoObra);
    }

    #[test]
    fn baremo_por_anio() {
        let mut bs = Baremos::default();
        bs.del_anio(2026).coste_senior = dec!(48.67);
        assert_eq!(bs.vigente(2027).unwrap().anio, 2026);
        let b27 = bs.del_anio(2027);
        assert_eq!(b27.coste_senior, dec!(48.67)); // copia del anterior
        b27.coste_senior = dec!(50.62);
        assert_eq!(bs.vigente(2027).unwrap().coste_senior, dec!(50.62));
        assert_eq!(bs.vigente(2026).unwrap().coste_senior, dec!(48.67));
        let json = serde_json::to_string(&bs).unwrap();
        let de: Baremos = serde_json::from_str(&json).unwrap();
        assert_eq!(de, bs);
        let parcial: Baremos = serde_json::from_str(
            r#"{"versiones":[{"anio":2026,"km":"0.26","visita_oca":"200","horas_memoria":{"base":"5"}}]}"#,
        )
        .unwrap();
        assert_eq!(parcial.versiones[0].km, dec!(0.26));
        assert_eq!(parcial.versiones[0].horas_memoria.base, dec!(5));
        assert!(parcial.versiones[0].visado_direccion_obra);
        assert_eq!(parcial.versiones[0].reparto_tramitacion.tecnico, dec!(30));
    }
}
