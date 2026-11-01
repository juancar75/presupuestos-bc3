// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-modelo
//!
//! Mediciones desde un **modelo BIM** (IFC, o Revit `.rvt` directamente)
//! para cualquier instalación: tuberías, conductos, bandejas, accesorios,
//! terminales, equipos, valvulería, aparatos sanitarios, PCI, iluminación…
//!
//! 1. Se leen los elementos de instalaciones del modelo ([`ifc::leer`] o
//!    [`revit::leer`]) a una lista neutra de [`Elemento`]s: clase, tipo,
//!    medida (Ø o sección), planta, sistema y longitud.
//! 2. [`a_presupuesto`] los agrupa: un capítulo por clase (o por sistema y
//!    clase), una partida por tipo y medida, en metros o unidades, con una
//!    línea de medición por planta y sistema. Las partidas quedan sin precio,
//!    listas para valorar como un ciego.
//!
//! Los elementos de arquitectura (muros, forjados…) se ignoran.

pub mod ifc;
#[cfg(feature = "revit")]
pub mod revit;

use ppto_core::medicion::{LineaMedicion, Medicion};
use ppto_core::{Concepto, Decimal, Presupuesto, redondear};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum ErrorModelo {
    #[error("no se pudo leer el fichero: {0}")]
    Fichero(#[from] std::io::Error),
    #[error("el fichero no es un IFC válido: {0}")]
    Ifc(String),
    #[error("no se pudo leer el modelo de Revit: {0}")]
    Revit(String),
    #[error("formato no admitido: {0} (use .ifc o .rvt)")]
    Formato(String),
    #[error("motor: {0}")]
    Motor(#[from] ppto_core::ErrorMotor),
}

/// Familia de elementos; da el capítulo y la unidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Clase {
    Tuberia,
    AccesorioTuberia,
    Conducto,
    AccesorioConducto,
    Bandeja,
    AccesorioBandeja,
    Cable,
    TerminalAire,
    Valvula,
    Equipo,
    Sanitario,
    Pci,
    Control,
    Electricidad,
    Iluminacion,
    /// Tramos genéricos (sin saber si son tubo o conducto).
    Tramo,
    /// Accesorios genéricos.
    Accesorio,
    Otros,
}

impl Clase {
    /// Nombre del capítulo.
    pub fn capitulo(self) -> &'static str {
        match self {
            Self::Tuberia => "Tuberías",
            Self::AccesorioTuberia => "Accesorios de tubería",
            Self::Conducto => "Conductos",
            Self::AccesorioConducto => "Accesorios de conducto",
            Self::Bandeja => "Bandejas y canalizaciones",
            Self::AccesorioBandeja => "Accesorios de bandeja",
            Self::Cable => "Cables",
            Self::TerminalAire => "Difusión de aire",
            Self::Valvula => "Valvulería y compuertas",
            Self::Equipo => "Equipos",
            Self::Sanitario => "Aparatos sanitarios",
            Self::Pci => "Protección contra incendios",
            Self::Control => "Control y detección",
            Self::Electricidad => "Electricidad",
            Self::Iluminacion => "Iluminación",
            Self::Tramo => "Tramos",
            Self::Accesorio => "Accesorios",
            Self::Otros => "Otros elementos",
        }
    }

    /// Nombre de una partida sin tipo.
    pub fn singular(self) -> &'static str {
        match self {
            Self::Tuberia => "Tubería",
            Self::AccesorioTuberia => "Accesorio de tubería",
            Self::Conducto => "Conducto",
            Self::AccesorioConducto => "Accesorio de conducto",
            Self::Bandeja => "Bandeja",
            Self::AccesorioBandeja => "Accesorio de bandeja",
            Self::Cable => "Cable",
            Self::TerminalAire => "Terminal de aire",
            Self::Valvula => "Válvula",
            Self::Equipo => "Equipo",
            Self::Sanitario => "Aparato sanitario",
            Self::Pci => "Elemento de PCI",
            Self::Control => "Elemento de control",
            Self::Electricidad => "Elemento eléctrico",
            Self::Iluminacion => "Luminaria",
            Self::Tramo => "Tramo",
            Self::Accesorio => "Accesorio",
            Self::Otros => "Elemento",
        }
    }

    /// Se mide en metros (tramos lineales).
    pub fn lineal(self) -> bool {
        matches!(
            self,
            Self::Tuberia | Self::Conducto | Self::Bandeja | Self::Cable | Self::Tramo
        )
    }

    pub fn unidad(self) -> &'static str {
        if self.lineal() { "m" } else { "ud" }
    }

    /// Clase de una entidad IFC (`IFCPIPESEGMENT`, `IFCPIPESEGMENTTYPE`…).
    /// `None`: no es de instalaciones.
    pub fn de_ifc(entidad: &str) -> Option<Self> {
        let e = entidad.to_ascii_uppercase();
        let e = e.strip_suffix("TYPE").unwrap_or(&e);
        Some(match e {
            "IFCPIPESEGMENT" => Self::Tuberia,
            "IFCPIPEFITTING" => Self::AccesorioTuberia,
            "IFCDUCTSEGMENT" => Self::Conducto,
            "IFCDUCTFITTING" | "IFCDUCTSILENCER" => Self::AccesorioConducto,
            "IFCCABLECARRIERSEGMENT" => Self::Bandeja,
            "IFCCABLECARRIERFITTING" => Self::AccesorioBandeja,
            "IFCCABLESEGMENT" => Self::Cable,
            "IFCCABLEFITTING" => Self::Electricidad,
            "IFCAIRTERMINAL" | "IFCAIRTERMINALBOX" => Self::TerminalAire,
            "IFCVALVE" | "IFCDAMPER" | "IFCFLOWCONTROLLER" => Self::Valvula,
            "IFCUNITARYEQUIPMENT"
            | "IFCFAN"
            | "IFCPUMP"
            | "IFCBOILER"
            | "IFCCHILLER"
            | "IFCCOIL"
            | "IFCHEATEXCHANGER"
            | "IFCAIRTOAIRHEATRECOVERY"
            | "IFCCOOLINGTOWER"
            | "IFCTANK"
            | "IFCFILTER"
            | "IFCHUMIDIFIER"
            | "IFCSPACEHEATER"
            | "IFCCOMPRESSOR"
            | "IFCCONDENSER"
            | "IFCEVAPORATOR"
            | "IFCEVAPORATIVECOOLER"
            | "IFCBURNER"
            | "IFCENGINE"
            | "IFCELECTRICGENERATOR"
            | "IFCTRANSFORMER"
            | "IFCELECTRICMOTOR"
            | "IFCSOLARDEVICE"
            | "IFCMEDICALDEVICE"
            | "IFCENERGYCONVERSIONDEVICE"
            | "IFCFLOWMOVINGDEVICE"
            | "IFCFLOWTREATMENTDEVICE"
            | "IFCFLOWSTORAGEDEVICE"
            | "IFCCHIMNEY" => Self::Equipo,
            "IFCSANITARYTERMINAL" | "IFCWASTETERMINAL" | "IFCSTACKTERMINAL" | "IFCINTERCEPTOR" => Self::Sanitario,
            "IFCFIRESUPPRESSIONTERMINAL" => Self::Pci,
            "IFCSENSOR"
            | "IFCALARM"
            | "IFCACTUATOR"
            | "IFCCONTROLLER"
            | "IFCFLOWINSTRUMENT"
            | "IFCUNITARYCONTROLELEMENT"
            | "IFCFLOWMETER"
            | "IFCDISTRIBUTIONCONTROLELEMENT" => Self::Control,
            "IFCOUTLET"
            | "IFCSWITCHINGDEVICE"
            | "IFCELECTRICDISTRIBUTIONBOARD"
            | "IFCJUNCTIONBOX"
            | "IFCPROTECTIVEDEVICE"
            | "IFCPROTECTIVEDEVICETRIPPINGUNIT"
            | "IFCELECTRICAPPLIANCE"
            | "IFCCOMMUNICATIONSAPPLIANCE"
            | "IFCAUDIOVISUALAPPLIANCE"
            | "IFCELECTRICFLOWSTORAGEDEVICE"
            | "IFCELECTRICTIMECONTROL"
            | "IFCMOBILETELECOMMUNICATIONSAPPLIANCE" => Self::Electricidad,
            "IFCLIGHTFIXTURE" | "IFCLAMP" => Self::Iluminacion,
            "IFCFLOWSEGMENT" => Self::Tramo,
            "IFCFLOWFITTING" => Self::Accesorio,
            "IFCFLOWTERMINAL"
            | "IFCDISTRIBUTIONFLOWELEMENT"
            | "IFCDISTRIBUTIONELEMENT"
            | "IFCDISTRIBUTIONCHAMBERELEMENT"
            | "IFCBUILDINGELEMENTPROXY"
            | "IFCFURNISHINGELEMENT"
            | "IFCDISTRIBUTIONPORT" => {
                // Puertos: no son elementos
                if e == "IFCDISTRIBUTIONPORT" {
                    return None;
                }
                Self::Otros
            }
            _ => return None,
        })
    }

    /// Las clases genéricas se concretan con su tipo.
    fn generica(self) -> bool {
        matches!(
            self,
            Self::Tramo | Self::Accesorio | Self::Otros | Self::Valvula | Self::Equipo
        )
    }
}

/// Un elemento de instalaciones del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elemento {
    pub clase: Clase,
    /// Familia y tipo (o nombre del tipo IFC); vacío si no se sabe.
    pub tipo: String,
    /// «Ø32», «300x200»…; vacío si no se sabe.
    pub medida: String,
    pub planta: String,
    pub sistema: String,
    /// Metros (solo tramos).
    pub longitud: Option<Decimal>,
    /// La longitud es aproximada (sacada de la caja que ocupa).
    pub aproximada: bool,
}

/// Lo leído del modelo.
#[derive(Debug, Clone, Default)]
pub struct Modelo {
    pub nombre: String,
    pub elementos: Vec<Elemento>,
    /// Elementos que no son de instalaciones (muros, forjados…).
    pub ignorados: usize,
    pub avisos: Vec<String>,
}

impl Modelo {
    /// Hay sistemas distintos en el modelo.
    pub fn tiene_sistemas(&self) -> bool {
        self.elementos.iter().any(|e| !e.sistema.is_empty())
    }

    pub fn tiene_plantas(&self) -> bool {
        self.elementos.iter().any(|e| !e.planta.is_empty())
    }

    /// Resumen por clase: (clase, elementos, metros).
    pub fn por_clase(&self) -> Vec<(Clase, usize, Decimal)> {
        let mut m: BTreeMap<Clase, (usize, Decimal)> = BTreeMap::new();
        for e in &self.elementos {
            let x = m.entry(e.clase).or_default();
            x.0 += 1;
            x.1 += e.longitud.unwrap_or_default();
        }
        m.into_iter().map(|(c, (n, l))| (c, n, l)).collect()
    }
}

/// Lee un modelo por su extensión (`.ifc` o `.rvt`).
pub fn leer(ruta: &std::path::Path) -> Result<Modelo, ErrorModelo> {
    let ext = ruta
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut m = match ext.as_str() {
        "ifc" => ifc::leer(&std::fs::read(ruta)?)?,
        #[cfg(feature = "revit")]
        "rvt" => revit::leer(ruta)?,
        _ => return Err(ErrorModelo::Formato(ext)),
    };
    if m.nombre.trim().is_empty() {
        m.nombre = ruta
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(m)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Opciones {
    /// Un capítulo por sistema y, dentro, uno por clase.
    pub por_sistema: bool,
    /// Una línea de medición por planta (si no, una sola línea).
    pub por_planta: bool,
}

/// Quita del nombre los identificadores de elemento de Revit
/// («NIRON_NP02_Codo-Socket:UT_NP02:6498021» → «NIRON_NP02_Codo-Socket:UT_NP02»,
/// «Pipe-6498003» → «»).
pub fn limpiar_tipo(t: &str) -> String {
    let mut s = t.trim().to_owned();
    // «…:123456»
    if let Some(i) = s.rfind(':')
        && s[i + 1..].chars().all(|c| c.is_ascii_digit())
        && i + 1 < s.len()
    {
        s.truncate(i);
    }
    // «Pipe-123456», «Duct-123»: nombre genérico del exportador
    if let Some(i) = s.rfind('-')
        && i + 1 < s.len()
        && s[i + 1..].chars().all(|c| c.is_ascii_digit())
        && !s[..i].contains(':')
        && !s[..i].contains(' ')
    {
        return String::new();
    }
    s
}

/// Medida legible en milímetros: Ø (dos lados iguales) o «ancho x alto».
pub fn medida_mm(a_m: Decimal, b_m: Decimal) -> String {
    let mm = |x: Decimal| redondear(x * Decimal::from(1000), 0);
    let (a, b) = (mm(a_m), mm(b_m));
    let (mayor, menor) = if a >= b { (a, b) } else { (b, a) };
    if mayor.is_zero() {
        return String::new();
    }
    // Iguales con un 10 % de margen (tubos un poco inclinados)
    if (mayor - menor) * Decimal::from(100) <= mayor * Decimal::from(10) {
        format!("Ø{mayor}")
    } else {
        format!("{mayor}x{menor}")
    }
}

/// Medida de un texto del modelo: «32 mmø», «Ø 32», «300x200», «DN25».
pub fn normalizar_medida(t: &str) -> String {
    let s = t.trim();
    if s.is_empty() {
        return String::new();
    }
    let limpio: String = s
        .replace("mmø", "")
        .replace("mm ø", "")
        .replace(['ø', 'Ø', '⌀'], "")
        .replace("mm", "")
        .replace(['×', 'X'], "x")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("");
    if limpio.is_empty() {
        return String::new();
    }
    let redondo = s.contains(['ø', 'Ø', '⌀'])
        || (!limpio.contains('x') && limpio.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ','));
    if redondo && !limpio.to_uppercase().starts_with("DN") {
        format!("Ø{limpio}")
    } else {
        limpio
    }
}

/// Grupo de elementos iguales (una partida).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Clave {
    sistema: String,
    clase: Clase,
    tipo: String,
    medida: String,
}

/// Convierte el modelo en un presupuesto sin precios.
pub fn a_presupuesto(m: &Modelo, o: Opciones) -> Result<Presupuesto, ErrorModelo> {
    let titulo = if m.nombre.trim().is_empty() {
        "Mediciones del modelo".to_owned()
    } else {
        m.nombre.clone()
    };
    let mut p = Presupuesto::new(titulo.clone(), "OBRA", titulo);
    let por_sistema = o.por_sistema && m.tiene_sistemas();
    // grupo → (planta, sistema) → (nº, metros, aproximadas)
    type Lineas = BTreeMap<(String, String), (usize, Decimal, usize)>;
    let mut grupos: BTreeMap<Clave, Lineas> = BTreeMap::new();
    for e in &m.elementos {
        let k = Clave {
            sistema: if por_sistema { e.sistema.clone() } else { String::new() },
            clase: e.clase,
            tipo: e.tipo.clone(),
            medida: e.medida.clone(),
        };
        let planta = if o.por_planta { e.planta.clone() } else { String::new() };
        let sistema = if por_sistema { String::new() } else { e.sistema.clone() };
        let x = grupos.entry(k).or_default().entry((planta, sistema)).or_default();
        x.0 += 1;
        x.1 += e.longitud.unwrap_or_default();
        x.2 += usize::from(e.aproximada);
    }
    let dm = p.decimales.medicion;
    let mut cap_sistema: BTreeMap<String, String> = BTreeMap::new();
    let mut cap_clase: BTreeMap<(String, Clase), String> = BTreeMap::new();
    let mut n_partida: BTreeMap<String, usize> = BTreeMap::new();
    for (k, lineas) in grupos {
        // Capítulo del sistema
        let padre = if por_sistema {
            let n = cap_sistema.len() + 1;
            let codigo = cap_sistema
                .entry(k.sistema.clone())
                .or_insert_with(|| format!("{n:02}"))
                .clone();
            if !p.conceptos.contains_key(&codigo) {
                let nombre = if k.sistema.is_empty() {
                    "Sin sistema"
                } else {
                    &k.sistema
                };
                p.insertar(Concepto::capitulo(codigo.clone(), nombre))?;
                p.colgar("OBRA", &codigo, None)?;
            }
            Some(codigo)
        } else {
            None
        };
        // Capítulo de la clase
        let clave_cap = (k.sistema.clone(), k.clase);
        let cap = match cap_clase.get(&clave_cap) {
            Some(c) => c.clone(),
            None => {
                let hermanos = cap_clase.keys().filter(|(s, _)| *s == k.sistema).count() + 1;
                let codigo = match &padre {
                    Some(s) => format!("{s}.{hermanos:02}"),
                    None => format!("{hermanos:02}"),
                };
                p.insertar(Concepto::capitulo(codigo.clone(), k.clase.capitulo()))?;
                p.colgar(padre.as_deref().unwrap_or("OBRA"), &codigo, None)?;
                cap_clase.insert(clave_cap, codigo.clone());
                codigo
            }
        };
        let i = n_partida.entry(cap.clone()).or_default();
        *i += 1;
        let codigo = format!("{cap}.{i:03}");
        let tipo = if k.tipo.is_empty() {
            k.clase.singular().to_owned()
        } else {
            k.tipo.clone()
        };
        let resumen = if k.medida.is_empty() {
            tipo
        } else {
            format!("{tipo} {}", k.medida)
        };
        let mut c = Concepto::partida(codigo.clone(), k.clase.unidad(), resumen, Vec::new());
        let total: usize = lineas.values().map(|x| x.0).sum();
        let aprox: usize = lineas.values().map(|x| x.2).sum();
        c.texto = Some(format!(
            "{}{}. Medición del modelo: {total} elemento(s){}.",
            k.clase.singular(),
            if k.medida.is_empty() {
                String::new()
            } else {
                format!(" {}", k.medida)
            },
            if aprox > 0 {
                format!(", {aprox} con longitud aproximada")
            } else {
                String::new()
            }
        ));
        p.insertar(c)?;
        let med: Vec<LineaMedicion> = lineas
            .into_iter()
            .map(|((planta, sistema), (n, l, _))| {
                let mut com: Vec<String> = [planta, sistema].into_iter().filter(|s| !s.is_empty()).collect();
                if k.clase.lineal() {
                    com.push(format!("{n} tramo(s)"));
                    LineaMedicion::normal(&com.join(" · "), None, Some(redondear(l, dm)), None, None)
                } else {
                    LineaMedicion::normal(&com.join(" · "), Some(Decimal::from(n)), None, None, None)
                }
            })
            .collect();
        p.colgar(&cap, &codigo, Some(Medicion::new(med)))?;
    }
    Ok(p)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn nombres_y_medidas() {
        assert_eq!(
            limpiar_tipo("NIRON_NP02_Codo-Socket:UT_NP02:6498021"),
            "NIRON_NP02_Codo-Socket:UT_NP02"
        );
        assert_eq!(limpiar_tipo("Pipe-6498003"), "");
        assert_eq!(limpiar_tipo("Rejilla 300x150"), "Rejilla 300x150");
        assert_eq!(limpiar_tipo("Codo 90-45"), "Codo 90-45");
        assert_eq!(medida_mm(dec!(0.032), dec!(0.032)), "Ø32");
        assert_eq!(medida_mm(dec!(0.0374), dec!(0.04)), "Ø40");
        assert_eq!(medida_mm(dec!(0.2), dec!(0.3)), "300x200");
        assert_eq!(normalizar_medida("32 mmø"), "Ø32");
        assert_eq!(normalizar_medida("300x200"), "300x200");
        assert_eq!(normalizar_medida("300 mm x 200 mm"), "300x200");
        assert_eq!(normalizar_medida("DN25"), "DN25");
        assert_eq!(Clase::de_ifc("IFCPIPESEGMENTTYPE"), Some(Clase::Tuberia));
        assert_eq!(Clase::de_ifc("IFCWALL"), None);
    }

    fn e(clase: Clase, tipo: &str, medida: &str, planta: &str, sistema: &str, l: Option<Decimal>) -> Elemento {
        Elemento {
            clase,
            tipo: tipo.into(),
            medida: medida.into(),
            planta: planta.into(),
            sistema: sistema.into(),
            longitud: l,
            aproximada: false,
        }
    }

    #[test]
    fn agrupa_en_capitulos_y_partidas() {
        let m = Modelo {
            nombre: "Prueba".into(),
            elementos: vec![
                e(Clase::Tuberia, "", "Ø32", "P1", "AF", Some(dec!(2.5))),
                e(Clase::Tuberia, "", "Ø32", "P1", "AF", Some(dec!(1.25))),
                e(Clase::Tuberia, "", "Ø32", "P2", "ACS", Some(dec!(4))),
                e(Clase::Tuberia, "", "Ø40", "P1", "AF", Some(dec!(3))),
                e(Clase::AccesorioTuberia, "Codo", "Ø32", "P1", "AF", None),
                e(Clase::AccesorioTuberia, "Codo", "Ø32", "P2", "AF", None),
                e(Clase::TerminalAire, "Difusor", "600x600", "P1", "Impulsión", None),
            ],
            ..Default::default()
        };
        let p = a_presupuesto(
            &m,
            Opciones {
                por_sistema: false,
                por_planta: true,
            },
        )
        .unwrap();
        assert_eq!(p.concepto("01").unwrap().resumen, "Tuberías");
        assert_eq!(p.concepto("01.001").unwrap().resumen, "Tubería Ø32");
        assert_eq!(p.concepto("01.001").unwrap().unidad, "m");
        let cant = p.cantidades_partidas().unwrap();
        assert_eq!(cant["01.001"], dec!(7.75));
        assert_eq!(cant["01.002"], dec!(3));
        assert_eq!(cant["02.001"], dec!(2));
        assert_eq!(p.concepto("02.001").unwrap().resumen, "Codo Ø32");
        assert_eq!(p.concepto("03.001").unwrap().unidad, "ud");
        // Una línea por planta y sistema
        assert_eq!(p.mediciones[&("01".into(), "01.001".into())].lineas.len(), 2);
        // Por sistema: AF, ACS, Impulsión
        let q = a_presupuesto(
            &m,
            Opciones {
                por_sistema: true,
                por_planta: false,
            },
        )
        .unwrap();
        let raiz: Vec<String> = q
            .concepto("OBRA")
            .unwrap()
            .descomposicion
            .iter()
            .map(|l| q.concepto(&l.hijo).unwrap().resumen.clone())
            .collect();
        assert_eq!(raiz, vec!["ACS", "AF", "Impulsión"]);
        assert_eq!(
            q.cantidades_partidas().unwrap().values().copied().sum::<Decimal>(),
            dec!(4) + dec!(3.75) + dec!(3) + dec!(2) + dec!(1)
        );
        q.validar().unwrap();
    }
}
