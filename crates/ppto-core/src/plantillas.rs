// SPDX-License-Identifier: GPL-3.0-or-later
//! Plantillas de obra: estructura de capítulos, partidas habituales (sin
//! precio, para valorar), observaciones particulares (capítulo 00),
//! exclusiones, legalización y capítulo de contradictorios (99).
//!
//! No llevan precios: se valoran con la biblioteca, las bases de precios o
//! la IA. Los textos son genéricos y se adaptan a cada obra.

use crate::Presupuesto;
use crate::error::ErrorMotor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plantilla {
    /// Climatización y ventilación de edificios (RITE).
    Climatizacion,
    /// Ventilación forzada de garaje (CTE DB-SI / DB-HS 3, UNE 100166).
    VentilacionGaraje,
    /// Protección contra incendios (RIPCI, UNE 23007).
    Pci,
}

impl Plantilla {
    pub const TODAS: [Plantilla; 3] = [Self::Climatizacion, Self::VentilacionGaraje, Self::Pci];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::Climatizacion => "Climatización (RITE)",
            Self::VentilacionGaraje => "Ventilación de garaje (CTE, UNE 100166)",
            Self::Pci => "Protección contra incendios (RIPCI)",
        }
    }

    /// Prefijo de capítulos y partidas cuando la obra junta varias plantillas.
    fn sigla(self) -> &'static str {
        match self {
            Self::Climatizacion => "CL",
            Self::VentilacionGaraje => "VG",
            Self::Pci => "PCI",
        }
    }

    /// Capítulos de la plantilla (número, resumen), para elegir cuáles entran.
    pub fn capitulos(self) -> Vec<(&'static str, &'static str)> {
        capitulos(self).into_iter().map(|(n, r, _)| (n, r)).collect()
    }
}

/// (código, unidad, resumen, texto)
type P = (&'static str, &'static str, &'static str, &'static str);

const OBS_COMUNES: [P; 3] = [
    (
        "OBS.01",
        "ud",
        "Criterio de valoración",
        "El presente presupuesto se ha valorado a medición sobre la documentación recibida. Las cantidades \
         definitivas serán las realmente ejecutadas y medidas en obra, a los precios unitarios ofertados.",
    ),
    (
        "OBS.02",
        "ud",
        "Exclusiones",
        "No se incluyen: ayudas de albañilería (apertura y tapado de huecos, rozas, bancadas), cerrajería y \
         soportes estructurales especiales, acometida eléctrica hasta cuadros, sellado de pasos con resistencia \
         al fuego, medios de elevación especiales, tasas y licencias, ni ningún trabajo no descrito \
         expresamente en las partidas.",
    ),
    (
        "OBS.03",
        "ud",
        "Condiciones",
        "Precios válidos 30 días. IVA no incluido. Plazo de ejecución según planificación de obra. \
         Los trabajos no previstos se valorarán como contradictorios antes de su ejecución.",
    ),
];

fn capitulos(t: Plantilla) -> Vec<(&'static str, &'static str, Vec<P>)> {
    match t {
        Plantilla::Climatizacion => vec![
            (
                "01",
                "Producción térmica",
                vec![
                    (
                        "01.01",
                        "ud",
                        "Unidad exterior bomba de calor",
                        "Unidad exterior de bomba de calor aire-agua o expansión directa, según proyecto, colocada sobre bancada con antivibratorios, conexionada y probada.",
                    ),
                    (
                        "01.02",
                        "ud",
                        "Depósito de inercia",
                        "Depósito de inercia para circuito de climatización, aislado, con valvulería de corte y vaciado.",
                    ),
                ],
            ),
            (
                "02",
                "Unidades terminales",
                vec![
                    (
                        "02.01",
                        "ud",
                        "Unidad interior tipo conductos",
                        "Unidad interior de conductos con mando, conexionada frigorífica, hidráulica y eléctricamente, con desagüe de condensados.",
                    ),
                    (
                        "02.02",
                        "ud",
                        "Fancoil o unidad de cassette",
                        "Unidad terminal de cassette o fancoil con válvula de 3 vías, conexionada y probada.",
                    ),
                ],
            ),
            (
                "03",
                "Red frigorífica y de agua",
                vec![
                    (
                        "03.01",
                        "m",
                        "Línea frigorífica aislada",
                        "Línea frigorífica de cobre deshidratado con aislamiento elastomérico, incluso soportes, soldaduras con nitrógeno, prueba de presión y vacío.",
                    ),
                    (
                        "03.02",
                        "m",
                        "Tubería de agua aislada",
                        "Tubería de acero o multicapa con aislamiento según RITE, incluso accesorios y soportes.",
                    ),
                ],
            ),
            (
                "04",
                "Conductos y difusión",
                vec![
                    (
                        "04.01",
                        "m2",
                        "Conducto de chapa o fibra",
                        "Conducto de chapa galvanizada aislada o panel de fibra, incluso piezas especiales y soportes.",
                    ),
                    (
                        "04.02",
                        "ud",
                        "Rejilla o difusor",
                        "Rejilla o difusor de impulsión o retorno con marco y compuerta de regulación.",
                    ),
                ],
            ),
            (
                "05",
                "Regulación y control",
                vec![(
                    "05.01",
                    "ud",
                    "Sistema de control",
                    "Sistema de control y regulación con termostatos de zona, cableado y programación.",
                )],
            ),
            (
                "06",
                "Desagües de condensados",
                vec![(
                    "06.01",
                    "m",
                    "Red de condensados",
                    "Tubería de PVC para desagüe de condensados con sifón, conectada a red de saneamiento.",
                )],
            ),
            (
                "07",
                "Pruebas y puesta en marcha",
                vec![(
                    "07.01",
                    "ud",
                    "Pruebas, equilibrado y puesta en marcha",
                    "Pruebas de estanqueidad, equilibrado de caudales, puesta en marcha y formación al usuario según RITE IT 2.",
                )],
            ),
            (
                "08",
                "Legalización",
                vec![
                    (
                        "08.01",
                        "ud",
                        "Memoria técnica o proyecto RITE",
                        "Memoria técnica o proyecto según la potencia térmica (RITE IT 1.1), con justificación RSIF del refrigerante.",
                    ),
                    (
                        "08.02",
                        "ud",
                        "Certificado de la instalación y tramitación",
                        "Certificado de la instalación térmica, manual de uso y mantenimiento y tramitación ante la comunidad autónoma. Tasas no incluidas.",
                    ),
                ],
            ),
        ],
        Plantilla::VentilacionGaraje => vec![
            (
                "01",
                "Ventiladores de extracción",
                vec![
                    (
                        "01.01",
                        "ud",
                        "Ventilador de extracción F400/120",
                        "Ventilador o caja de ventilación certificado F400/120 para extracción de humos y CO, con antivibratorios y conexión.",
                    ),
                    (
                        "01.02",
                        "ud",
                        "Ventilador de aportación",
                        "Ventilador de aportación de aire, en su caso, con soportes y conexión.",
                    ),
                ],
            ),
            (
                "02",
                "Conductos",
                vec![(
                    "02.01",
                    "m2",
                    "Conducto de chapa galvanizada",
                    "Conducto de chapa de acero galvanizado según UNE-EN 1505/1506, incluso piezas especiales y soportes.",
                )],
            ),
            (
                "03",
                "Rejillas y compuertas",
                vec![
                    (
                        "03.01",
                        "ud",
                        "Rejilla de extracción",
                        "Rejilla de extracción de acero con regulación, colocada en conducto.",
                    ),
                    (
                        "03.02",
                        "ud",
                        "Compuerta cortafuegos",
                        "Compuerta cortafuegos en paso de sector, con fusible o actuador según proyecto.",
                    ),
                ],
            ),
            (
                "04",
                "Detección de CO",
                vec![
                    (
                        "04.01",
                        "ud",
                        "Central de detección de CO",
                        "Central de detección de monóxido de carbono con salidas de maniobra para los ventiladores.",
                    ),
                    (
                        "04.02",
                        "ud",
                        "Detector de CO",
                        "Detector de CO colocado y conexionado (UNE 23300), uno cada 200 m² aproximadamente según fabricante.",
                    ),
                ],
            ),
            (
                "05",
                "Cuadros y maniobra",
                vec![
                    (
                        "05.01",
                        "ud",
                        "Cuadro de ventilación con variadores",
                        "Cuadro eléctrico de mando y protección de ventiladores, con maniobra automática por CO y pulsador de bomberos.",
                    ),
                    (
                        "05.02",
                        "m",
                        "Línea resistente al fuego",
                        "Línea de alimentación a ventiladores con cable resistente al fuego, en bandeja o tubo.",
                    ),
                ],
            ),
            (
                "06",
                "Pruebas y puesta en marcha",
                vec![(
                    "06.01",
                    "ud",
                    "Medición de caudales y puesta en marcha",
                    "Medición de caudales, pruebas de funcionamiento con la detección de CO y puesta en marcha.",
                )],
            ),
            (
                "07",
                "Legalización",
                vec![(
                    "07.01",
                    "ud",
                    "Certificado y documentación",
                    "Certificado de la instalación, documentación as-built y tramitación. Tasas no incluidas.",
                )],
            ),
        ],
        Plantilla::Pci => vec![
            (
                "01",
                "Detección y alarma",
                vec![
                    (
                        "01.01",
                        "ud",
                        "Central de detección",
                        "Central de detección de incendios analógica o convencional según UNE 23007-2, con baterías.",
                    ),
                    (
                        "01.02",
                        "ud",
                        "Detector óptico de humos",
                        "Detector óptico de humos con base, colocado y conexionado (UNE 23007-14).",
                    ),
                    (
                        "01.03",
                        "ud",
                        "Pulsador de alarma",
                        "Pulsador manual de alarma rearmable, colocado y conexionado.",
                    ),
                    (
                        "01.04",
                        "ud",
                        "Sirena",
                        "Sirena de alarma óptico-acústica, colocada y conexionada.",
                    ),
                    (
                        "01.05",
                        "m",
                        "Cableado de lazo",
                        "Cableado de lazo con cable resistente al fuego en tubo, incluso conexiones.",
                    ),
                ],
            ),
            (
                "02",
                "Extintores",
                vec![
                    (
                        "02.01",
                        "ud",
                        "Extintor de polvo ABC 6 kg",
                        "Extintor de polvo ABC 21A-113B de 6 kg con soporte, colocado y señalizado.",
                    ),
                    (
                        "02.02",
                        "ud",
                        "Extintor de CO2 5 kg",
                        "Extintor de CO2 de 5 kg 113B con soporte, colocado y señalizado.",
                    ),
                ],
            ),
            (
                "03",
                "Red de BIE",
                vec![
                    (
                        "03.01",
                        "ud",
                        "BIE 25 mm",
                        "Boca de incendio equipada de 25 mm con armario, manguera semirrígida de 20 m y manómetro.",
                    ),
                    (
                        "03.02",
                        "m",
                        "Tubería de acero para PCI",
                        "Tubería de acero negro o galvanizado para red de BIE, pintada, incluso accesorios y soportes.",
                    ),
                ],
            ),
            (
                "04",
                "Abastecimiento de agua",
                vec![(
                    "04.01",
                    "ud",
                    "Grupo de presión contra incendios",
                    "Grupo de presión contra incendios según UNE 23500, con bomba principal, jockey y cuadro.",
                )],
            ),
            (
                "05",
                "Señalización",
                vec![
                    (
                        "05.01",
                        "ud",
                        "Señal de extinción",
                        "Señal fotoluminiscente de equipo de extinción (UNE 23033), colocada.",
                    ),
                    (
                        "05.02",
                        "ud",
                        "Señal de evacuación",
                        "Señal fotoluminiscente de evacuación (UNE 23034), colocada.",
                    ),
                ],
            ),
            (
                "06",
                "Pruebas y puesta en marcha",
                vec![(
                    "06.01",
                    "ud",
                    "Pruebas y puesta en marcha",
                    "Pruebas de funcionamiento de detección y extinción, prueba hidráulica de la red y puesta en marcha.",
                )],
            ),
            (
                "07",
                "Legalización",
                vec![
                    (
                        "07.01",
                        "ud",
                        "Certificado de la instalación PCI",
                        "Certificado del instalador habilitado según RIPCI, documentación y tramitación ante la comunidad autónoma. Tasas no incluidas.",
                    ),
                    (
                        "07.02",
                        "ud",
                        "Inspección inicial OCA",
                        "Inspección por organismo de control, cuando la exija el RIPCI.",
                    ),
                ],
            ),
        ],
    }
}

/// Presupuesto nuevo con la estructura de la plantilla.
pub fn crear(t: Plantilla, nombre: &str) -> Result<Presupuesto, ErrorMotor> {
    let todos: Vec<&str> = t.capitulos().into_iter().map(|(n, _)| n).collect();
    crear_con(&[(t, todos)], nombre)
}

/// Presupuesto nuevo con los capítulos elegidos de una o varias plantillas
/// (p. ej. climatización y PCI en la misma oferta). Con una sola plantilla
/// los códigos son los de siempre (C01, P01.01); con varias, cada
/// especialidad lleva su prefijo (CCL01 y PCL01.01, CPCI01 y PPCI01.01) y sus
/// capítulos se ordenan seguidos. Siempre van las observaciones (C00) y los
/// contradictorios (C99).
pub fn crear_con(elegidos: &[(Plantilla, Vec<&str>)], nombre: &str) -> Result<Presupuesto, ErrorMotor> {
    let mut p = Presupuesto::new(nombre, "OBRA", nombre);
    let poner =
        |p: &mut Presupuesto, cap: &str, codigo: String, (_, ud, resumen, texto): P| -> Result<(), ErrorMotor> {
            p.nueva_partida(cap, &codigo, ud, resumen)?;
            if let Some(c) = p.conceptos.get_mut(&codigo) {
                c.texto = Some(texto.split_whitespace().collect::<Vec<_>>().join(" "));
            }
            Ok(())
        };
    p.nuevo_capitulo("OBRA", "C00", "Observaciones particulares de esta oferta")?;
    for o in OBS_COMUNES {
        poner(&mut p, "C00", o.0.to_owned(), o)?;
    }
    let varias = elegidos.len() > 1;
    for (t, caps) in elegidos {
        let sigla = if varias { t.sigla() } else { "" };
        for (n, resumen, partidas) in capitulos(*t) {
            if !caps.contains(&n) {
                continue;
            }
            let cap = format!("C{sigla}{n}");
            let resumen = if varias {
                format!("{} · {resumen}", t.nombre())
            } else {
                resumen.to_owned()
            };
            p.nuevo_capitulo("OBRA", &cap, &resumen)?;
            for x in partidas {
                let codigo = if x.0.starts_with("OBS") {
                    x.0.to_owned()
                } else {
                    format!("P{sigla}{}", x.0)
                };
                poner(&mut p, &cap, codigo, x)?;
            }
        }
    }
    p.nuevo_capitulo("OBRA", "C99", "Contradictorios")?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plantillas_validas_y_sin_precios() {
        for t in Plantilla::TODAS {
            let p = crear(t, t.nombre()).unwrap();
            p.validar().unwrap();
            assert_eq!(p.pem().unwrap(), crate::Decimal::ZERO, "{}: sin precios", t.nombre());
            let caps = &p.conceptos["OBRA"].descomposicion;
            assert_eq!(caps.first().unwrap().hijo, "C00");
            assert_eq!(caps.last().unwrap().hijo, "C99");
            assert!(p.conceptos.contains_key("OBS.02"), "exclusiones");
            assert!(
                p.conceptos
                    .values()
                    .any(|c| c.resumen.to_lowercase().contains("certificado")),
                "{}: legalización",
                t.nombre()
            );
            // Todas las partidas tienen texto descriptivo
            assert!(
                p.conceptos
                    .values()
                    .filter(|c| c.naturaleza == crate::Naturaleza::Partida)
                    .all(|c| c.texto.as_deref().is_some_and(|t| t.len() > 20))
            );
        }
    }

    #[test]
    fn varias_plantillas_y_capitulos_elegidos() {
        let cl = Plantilla::Climatizacion.capitulos();
        let pci = Plantilla::Pci.capitulos();
        assert!(cl.len() > 2 && pci.len() > 2);
        let p = crear_con(
            &[
                (Plantilla::Climatizacion, vec![cl[0].0, cl[1].0]),
                (Plantilla::Pci, pci.iter().map(|c| c.0).collect()),
            ],
            "Clima y PCI",
        )
        .unwrap();
        p.validar().unwrap();
        let caps: Vec<&str> = p.conceptos["OBRA"]
            .descomposicion
            .iter()
            .map(|l| l.hijo.as_str())
            .collect();
        assert_eq!(caps.len(), 2 + 2 + pci.len());
        assert_eq!(caps[0], "C00");
        assert_eq!(caps[1], format!("CCL{}", cl[0].0));
        assert_eq!(*caps.last().unwrap(), "C99");
        assert!(caps.contains(&format!("CPCI{}", pci[0].0).as_str()));
        assert!(
            !caps.contains(&format!("CCL{}", cl[2].0).as_str()),
            "capítulo no elegido"
        );
        assert!(
            p.conceptos[&format!("CPCI{}", pci[0].0)]
                .resumen
                .starts_with("Protección contra incendios")
        );
        // Una sola plantilla: los códigos de siempre
        let q = crear(Plantilla::Pci, "PCI").unwrap();
        assert!(q.conceptos.contains_key(&format!("C{}", pci[0].0)));
    }
}
