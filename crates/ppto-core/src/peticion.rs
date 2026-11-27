// SPDX-License-Identifier: GPL-3.0-or-later
//! **Petición de precios a proveedores**: lo que en la obra está sin precio,
//! agrupado por fabricante o proveedor, con su referencia y la cantidad
//! total que necesita la obra.
//!
//! Entra:
//! - cada **recurso** (material, maquinaria, otros o subcontrata) a precio 0
//!   que usa alguna partida, con la cantidad que sale de la explosión;
//! - cada **partida sin descompuesto** y a 0 (típica del ciego del cliente),
//!   con su medición total y su texto largo.
//!
//! El proveedor sale de la tarifa o del origen del precio del concepto
//! ([`crate::pedidos::proveedor`]); lo que no tiene va en una petición
//! «sin proveedor» para asignarlo. Cada petición solo lleva **sus** líneas:
//! a un fabricante nunca le llega lo de otro.
//!
//! Cada línea lleva además su **tipo de producto** ([`tipo_producto`]:
//! recuperadores, rejillas y difusores, VRF…) para poder pedir lo mismo a
//! varias marcas y comparar ([`por_tipo`]).

use crate::concepto::Naturaleza;
use crate::error::ErrorMotor;
use crate::pedidos::{proveedor, usos};
use crate::presupuesto::Presupuesto;
use crate::{Decimal, emparejar};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct LineaPeticion {
    pub codigo: String,
    /// Referencia del artículo (tarifa u oferta), si se conoce.
    pub referencia: Option<String>,
    pub resumen: String,
    /// Texto largo (descripción completa), si lo tiene.
    pub texto: String,
    pub unidad: String,
    pub cantidad: Decimal,
    /// Tipo de producto reconocido por el texto ([`tipo_producto`]).
    pub tipo: &'static str,
    /// Partida sin descompuesto (se pide la unidad de obra entera).
    pub es_partida: bool,
    /// Variante: la añade el técnico para que se valore como alternativa
    /// (puede tener ya precio).
    pub variante: bool,
    /// El proveedor viene de su tarifa (no se puede cambiar desde aquí).
    pub de_tarifa: bool,
    /// Partidas de la obra que lo usan (solo para uso interno).
    pub partidas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Peticion {
    /// Fabricante o proveedor ("" = sin asignar).
    pub proveedor: String,
    pub lineas: Vec<LineaPeticion>,
}

impl Peticion {
    pub fn nombre(&self) -> &str {
        if self.proveedor.is_empty() {
            "(sin proveedor)"
        } else {
            &self.proveedor
        }
    }
}

fn crear_linea(
    p: &Presupuesto,
    codigo: &str,
    cantidad: Decimal,
    es_partida: bool,
    partidas: Vec<String>,
) -> Result<LineaPeticion, ErrorMotor> {
    let c = p.concepto(codigo)?;
    let referencia = c
        .tarifa
        .as_ref()
        .and_then(|t| t.referencia.clone())
        .or_else(|| c.origen.as_ref().and_then(|o| o.referencia.clone()))
        .filter(|r| !r.trim().is_empty());
    Ok(LineaPeticion {
        codigo: codigo.to_owned(),
        referencia,
        resumen: c.resumen.clone(),
        texto: c.texto.clone().unwrap_or_default().trim().to_owned(),
        unidad: c.unidad.clone(),
        cantidad,
        tipo: tipo_producto(&c.resumen, c.texto.as_deref().unwrap_or("")),
        es_partida,
        variante: false,
        de_tarifa: c.tarifa.as_ref().is_some_and(|t| !t.fabricante.is_empty()),
        partidas,
    })
}

/// Línea de **variante**: una partida (o recurso) de la obra, tenga o no
/// precio, para que el proveedor la valore como alternativa. Lleva la
/// medición total de la obra.
pub fn variante(p: &Presupuesto, codigo: &str) -> Result<LineaPeticion, ErrorMotor> {
    let c = p.concepto(codigo)?;
    let (cantidad, es_partida) = if c.naturaleza.es_basico() {
        let e = p.explosion_recursos()?;
        let q = e.recursos.iter().find(|r| r.codigo == codigo).map(|r| r.cantidad);
        (q.unwrap_or_default(), false)
    } else if c.naturaleza == Naturaleza::Partida {
        (p.cantidades_partidas()?.get(codigo).copied().unwrap_or_default(), true)
    } else {
        return Err(ErrorMotor::ValorInvalido(format!(
            "«{codigo}» no es una partida ni un recurso"
        )));
    };
    let mut l = crear_linea(p, codigo, cantidad, es_partida, vec![codigo.to_owned()])?;
    l.variante = true;
    Ok(l)
}

/// Peticiones de precio de la obra, una por proveedor, en orden alfabético;
/// la de «sin proveedor», al final.
pub fn peticiones(p: &Presupuesto) -> Result<Vec<Peticion>, ErrorMotor> {
    let e = p.explosion_recursos()?;
    let usos = usos(p)?;
    let mut m: BTreeMap<(bool, String), Vec<LineaPeticion>> = BTreeMap::new();
    let mut linea =
        |codigo: &str, cantidad: Decimal, es_partida: bool, partidas: Vec<String>| -> Result<(), ErrorMotor> {
            let prov = proveedor(p, codigo);
            let l = crear_linea(p, codigo, cantidad, es_partida, partidas)?;
            m.entry((prov.is_empty(), prov)).or_default().push(l);
            Ok(())
        };
    for r in &e.recursos {
        let pide = matches!(
            r.naturaleza,
            Naturaleza::Material | Naturaleza::Maquinaria | Naturaleza::Otros | Naturaleza::Subcontrata
        );
        if !pide || r.cantidad.is_zero() || !r.precio.is_zero() {
            continue;
        }
        let partidas = usos
            .get(&r.codigo)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        linea(&r.codigo, r.cantidad, false, partidas)?;
    }
    let cantidades = p.cantidades_partidas()?;
    for codigo in emparejar::sin_precio(p) {
        let q = cantidades.get(&codigo).copied().unwrap_or_default();
        if q.is_zero() {
            continue;
        }
        linea(&codigo, q, true, vec![codigo.clone()])?;
    }
    Ok(m.into_iter()
        .map(|((_, proveedor), mut lineas)| {
            lineas.sort_by(|a, b| a.codigo.cmp(&b.codigo));
            Peticion { proveedor, lineas }
        })
        .collect())
}

/// Tipo para lo que no se reconoce.
pub const OTROS: &str = "Otros";

/// Tipos de producto y las palabras que los reconocen, de más concreto a
/// más general (un recuperador lleva ventiladores, pero es un recuperador).
/// Las palabras van en minúsculas y sin tildes; casan con el principio de
/// una palabra del texto («rejilla» casa con «rejillas»), salvo las de tres
/// letras o menos, que tienen que ser la palabra entera («BIE», «UTA»).
pub const TIPOS: &[(&str, &[&str])] = &[
    ("Recuperadores de calor", &["recuperador", "doble flujo"]),
    (
        "Climatizadoras (UTA)",
        &["climatizador", "uta", "unidad de tratamiento"],
    ),
    ("Fancoils", &["fancoil", "fan coil", "fan-coil"]),
    (
        "Enfriadoras y bombas de calor",
        &["enfriadora", "bomba de calor", "aerotermi", "planta enfriadora"],
    ),
    (
        "Equipos VRF y split",
        &[
            "vrf",
            "vrv",
            "split",
            "multisplit",
            "cassette",
            "unidad interior",
            "unidad exterior",
            "unidad condensadora",
        ],
    ),
    (
        "Suelo radiante",
        &["suelo radiante", "colector de suelo", "panel aislante", "tetones"],
    ),
    (
        "Rejillas y difusores",
        &[
            "rejilla",
            "difusor",
            "tobera",
            "plenum",
            "boca de",
            "lineal de impulsion",
        ],
    ),
    ("Compuertas", &["compuerta", "regulador de caudal", "caja de caudal"]),
    (
        "Ventiladores y extractores",
        &[
            "ventilador",
            "extractor",
            "caja de ventilacion",
            "cajas de ventilacion",
            "aspirador",
        ],
    ),
    (
        "Conductos",
        &["conducto", "climaver", "chapa galvanizada", "helicoidal", "flexible"],
    ),
    (
        "Detección de incendios",
        &[
            "detector",
            "deteccion",
            "pulsador",
            "sirena",
            "central de alarma",
            "modulo",
        ],
    ),
    (
        "Extinción de incendios",
        &[
            "extintor",
            "bie",
            "rociador",
            "sprinkler",
            "hidrante",
            "columna seca",
            "grupo contra incendios",
        ],
    ),
    (
        "Señalización y emergencia",
        &["senal", "senalizacion", "emergencia", "fotoluminiscente"],
    ),
    (
        "Bombas y grupos de presión",
        &["bomba", "circulador", "grupo de presion", "electrobomba"],
    ),
    (
        "Valvulería",
        &[
            "valvula",
            "llave",
            "filtro en y",
            "antirretorno",
            "vaso de expansion",
            "purgador",
        ],
    ),
    ("Aislamiento", &["aislamiento", "coquilla", "armaflex", "aislante"]),
    (
        "Tuberías",
        &[
            "tuberia",
            "tubo",
            "cobre",
            "multicapa",
            "polipropileno",
            "ppr",
            "pex",
            "acero negro",
        ],
    ),
    (
        "Control y regulación",
        &[
            "termostato",
            "sonda",
            "regulacion",
            "control",
            "bms",
            "controlador",
            "actuador",
        ],
    ),
    (
        "Electricidad",
        &[
            "cable",
            "cuadro",
            "bandeja",
            "interruptor",
            "magnetotermico",
            "diferencial",
        ],
    ),
];

/// Texto en minúsculas, sin tildes y con solo letras y números separados
/// por un espacio (con espacios al principio y al final).
fn normal(t: &str) -> String {
    let mut s = String::from(" ");
    for c in t.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        };
        if c.is_alphanumeric() {
            s.push(c);
        } else if !s.ends_with(' ') {
            s.push(' ');
        }
    }
    if !s.ends_with(' ') {
        s.push(' ');
    }
    s
}

fn tipo_de(texto: &str) -> Option<&'static str> {
    let t = normal(texto);
    TIPOS.iter().find_map(|(tipo, palabras)| {
        palabras
            .iter()
            .any(|w| {
                let w = normal(w);
                let w = w.trim_end();
                if w.trim().len() <= 3 {
                    t.contains(&format!("{w} "))
                } else {
                    t.contains(w)
                }
            })
            .then_some(*tipo)
    })
}

/// Tipo de producto por el resumen o, si no se reconoce, por el texto largo.
pub fn tipo_producto(resumen: &str, texto: &str) -> &'static str {
    tipo_de(resumen).or_else(|| tipo_de(texto)).unwrap_or(OTROS)
}

/// Todas las líneas por tipo de producto (con el proveedor que tengan),
/// para pedir el mismo tipo a varias marcas.
pub fn por_tipo(peticiones: &[Peticion]) -> BTreeMap<&'static str, Vec<(String, LineaPeticion)>> {
    let mut m: BTreeMap<&'static str, Vec<(String, LineaPeticion)>> = BTreeMap::new();
    for x in peticiones {
        for l in &x.lineas {
            m.entry(l.tipo).or_default().push((x.proveedor.clone(), l.clone()));
        }
    }
    for v in m.values_mut() {
        v.sort_by(|a, b| a.1.codigo.cmp(&b.1.codigo));
    }
    m
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::{Concepto, LineaDescomposicion, OrigenPrecio};
    use rust_decimal_macros::dec;

    fn obra() -> Presupuesto {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        for (c, u, r, n, pr) in [
            ("MT.A", "ud", "Rejilla 400x200", Naturaleza::Material, dec!(0)),
            ("MT.B", "m", "Tubo cobre 1/2\"", Naturaleza::Material, dec!(0)),
            ("MT.C", "ud", "Soporte", Naturaleza::Material, dec!(3)),
            ("MO.1", "h", "Oficial", Naturaleza::ManoObra, dec!(0)),
        ] {
            p.conceptos.insert(c.into(), Concepto::basico(c, u, r, n, pr));
        }
        let mut pa = Concepto::basico("P1", "ud", "Partida 1", Naturaleza::Partida, dec!(0));
        pa.descomposicion = vec![
            LineaDescomposicion::new("MT.A", dec!(1)),
            LineaDescomposicion::new("MT.B", dec!(2.5)),
            LineaDescomposicion::new("MT.C", dec!(1)),
            LineaDescomposicion::new("MO.1", dec!(1)),
        ];
        p.conceptos.insert("P1".into(), pa);
        let mut ciego = Concepto::basico(
            "P2",
            "ud",
            "Recuperador de calor 1000 m3/h",
            Naturaleza::Partida,
            dec!(0),
        );
        ciego.texto = Some("Recuperador con by-pass y filtros F7.".into());
        p.conceptos.insert("P2".into(), ciego);
        let mut cap = Concepto::basico("C01", "", "Clima", Naturaleza::Capitulo, dec!(0));
        cap.descomposicion = vec![
            LineaDescomposicion::new("P1", dec!(4)),
            LineaDescomposicion::new("P2", dec!(2)),
        ];
        p.conceptos.insert("C01".into(), cap);
        p.conceptos.get_mut("OBRA").unwrap().descomposicion = vec![LineaDescomposicion::new("C01", dec!(1))];
        p
    }

    #[test]
    fn solo_lo_sin_precio_y_cada_proveedor_lo_suyo() {
        let mut p = obra();
        p.fijar_origen(
            "MT.A",
            Some(OrigenPrecio {
                fabricante: "Koolair".into(),
                referencia: Some("20-H 400x200".into()),
                ..Default::default()
            }),
            false,
        )
        .unwrap();
        let v = peticiones(&p).unwrap();
        // Koolair primero; lo sin proveedor al final
        assert_eq!(
            v.iter().map(Peticion::nombre).collect::<Vec<_>>(),
            ["KOOLAIR", "(sin proveedor)"]
        );
        let k = &v[0];
        assert_eq!(k.lineas.len(), 1);
        assert_eq!(k.lineas[0].codigo, "MT.A");
        assert_eq!(k.lineas[0].cantidad, dec!(4));
        assert_eq!(k.lineas[0].referencia.as_deref(), Some("20-H 400x200"));
        let s = &v[1];
        // Ni la mano de obra ni lo que ya tiene precio
        assert_eq!(
            s.lineas.iter().map(|l| l.codigo.as_str()).collect::<Vec<_>>(),
            ["MT.B", "P2"]
        );
        assert_eq!(s.lineas[0].cantidad, dec!(10));
        assert!(s.lineas[1].es_partida);
        assert_eq!(s.lineas[1].cantidad, dec!(2));
        assert_eq!(s.lineas[1].texto, "Recuperador con by-pass y filtros F7.");
        // Con precio deja de pedirse
        p.fijar_precio("MT.B", dec!(5)).unwrap();
        let v = peticiones(&p).unwrap();
        assert!(v.iter().all(|x| x.lineas.iter().all(|l| l.codigo != "MT.B")));
    }

    #[test]
    fn reconoce_el_tipo_de_producto() {
        for (t, esperado) in [
            (
                "Recuperador de calor con ventiladores EC 1000 m³/h",
                "Recuperadores de calor",
            ),
            ("Rejillas de retorno 400x200", "Rejillas y difusores"),
            ("Unidad interior VRF tipo cassette 4 vías", "Equipos VRF y split"),
            ("Tubería de cobre 1/2\" aislada", "Tuberías"),
            ("BIE de 25 mm con armario", "Extinción de incendios"),
            ("Detector óptico de humos", "Detección de incendios"),
            ("Caja de ventilación CVHT 1,1 kW", "Ventiladores y extractores"),
            ("Bomba de calor aire-agua", "Enfriadoras y bombas de calor"),
            ("Bien de equipo sin clasificar", OTROS),
        ] {
            assert_eq!(tipo_producto(t, ""), esperado, "{t}");
        }
        assert_eq!(
            tipo_producto("Equipo de ventilación doble flujo", ""),
            "Recuperadores de calor"
        );
        assert_eq!(
            tipo_producto("Equipo ref. A-12", "Recuperador entálpico"),
            "Recuperadores de calor"
        );
        let p = obra();
        let v = peticiones(&p).unwrap();
        let t = por_tipo(&v);
        assert_eq!(t["Recuperadores de calor"].len(), 1);
        assert_eq!(t["Rejillas y difusores"][0].1.codigo, "MT.A");
    }

    #[test]
    fn variantes_con_su_medicion() {
        let p = obra();
        let l = variante(&p, "P1").unwrap();
        assert!(l.variante && l.es_partida);
        assert_eq!(l.cantidad, dec!(4));
        let r = variante(&p, "MT.C").unwrap();
        assert!(!r.es_partida);
        assert_eq!(r.cantidad, dec!(4));
        assert!(variante(&p, "C01").is_err());
    }
}
