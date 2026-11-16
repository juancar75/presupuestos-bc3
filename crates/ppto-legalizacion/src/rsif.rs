// SPDX-License-Identifier: GPL-3.0-or-later
//! Clasificación de la parte frigorífica de una instalación térmica según el
//! RSIF (RD 552/2019), que se comprueba **además** del umbral de 70 kW del
//! RITE: una instalación de menos de 70 kW térmicos puede necesitar proyecto
//! frigorífico, y una con A2L puede ir con memoria pero con la justificación
//! de seguridad firmada por un técnico competente.
//!
//! Criterios (art. 8, art. 21.3–21.5, IF-04 apéndices 3 y 4, IF-20 y la
//! consulta 74 de la guía técnica ministerial):
//!
//! | Supuesto | Documentación RSIF | Intervención |
//! |---|---|---|
//! | Sistema indirecto cerrado con equipos compactos L2/L3 (IF-20) | la del tramo RITE | mantenimiento del primario por empresa del apdo. 4.2 |
//! | L1 compacto con condensador incorporado (art. 8) | nivel 1, cualquier potencia | — |
//! | L1 con ≤ 30 kW por sistema y ≤ 100 kW en total | nivel 1 | — |
//! | A2L con ≤ 30 / ≤ 100 kW y justificación ordinaria (ap. 3) | memoria y documentación específica | la firma el instalador habilitado |
//! | A2L con ≤ 30 / ≤ 100 kW y apéndice 4 | memoria y justificación | técnico competente |
//! | A2L con ≤ 30 / ≤ 100 kW y análisis de riesgos (art. 21.3) | memoria y documentación del 21.3 | técnico competente y empresa de nivel 2 |
//! | Resto (L1 o A2L por encima de 30 / 100 kW, L2 no A2L, L3 sin IF-20) | **proyecto** (nivel 2 general) | técnico competente y dirección técnica |
//!
//! Las potencias de 30 y 100 kW son **eléctricas instaladas en compresores**
//! (por sistema y del conjunto), no potencias térmicas.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Grupo del refrigerante a efectos del RSIF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Grupo {
    /// L1 (R410A, R134a, R744…).
    #[default]
    L1,
    /// L2 de baja inflamabilidad (R32, R454B, R1234yf…).
    A2l,
    /// L2 distinto de A2L.
    L2,
    /// L3 (R290, R600a…).
    L3,
}

impl Grupo {
    /// Grupo de un refrigerante por su nombre («R32», «R-290»…). `None` si no se conoce.
    pub fn de_refrigerante(nombre: &str) -> Option<Self> {
        let n: String = nombre
            .to_uppercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        Some(match n.as_str() {
            "R32" | "R454B" | "R454C" | "R452B" | "R1234YF" | "R1234ZE" | "R1234ZEE" => Self::A2l,
            "R290" | "R600A" | "R1270" | "R600" => Self::L3,
            "R717" => Self::L2,
            "R410A" | "R407C" | "R134A" | "R744" | "R513A" | "R404A" | "R448A" | "R449A" | "R22" => Self::L1,
            _ => return None,
        })
    }
}

/// Cómo se justifica la carga de un A2L.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum JustificacionCarga {
    /// Vía ordinaria (IF-04, apéndice 3).
    #[default]
    Ordinaria,
    /// Medidas del apéndice 4 de la IF-04 (consulta 74).
    Apendice4,
    /// Análisis de riesgos del artículo 21.3.
    AnalisisRiesgos,
}

/// Datos frigoríficos de la instalación.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DatosRsif {
    pub grupo: Grupo,
    /// Equipo compacto con condensador incorporado (enfriadora, bomba de calor
    /// monobloc). `false` = circuito construido o interconectado en obra (split, VRF).
    pub compacto: bool,
    /// Sistema indirecto cerrado (el refrigerante no sale del equipo; se
    /// distribuye agua u otro fluido secundario).
    pub indirecto_cerrado: bool,
    /// Potencia eléctrica instalada en compresores del mayor sistema (kW).
    pub compresores_sistema_max_kw: Decimal,
    /// Suma de la potencia eléctrica instalada en compresores de todos los sistemas (kW).
    pub compresores_total_kw: Decimal,
    /// Enfría alguna cámara de atmósfera artificial.
    pub camara_atmosfera_artificial: bool,
    pub justificacion_a2l: JustificacionCarga,
}

/// Régimen RSIF resultante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Regimen {
    /// Condiciones especiales de la IF-20.
    If20,
    Nivel1,
    /// A2L por la vía simplificada (art. 21.4).
    A2lSimplificado,
    /// Nivel 2 general: proyecto y dirección técnica.
    Nivel2,
}

/// Documento que exige el RSIF (se suma al que exija el RITE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentoRsif {
    /// Lo que pida el RITE por potencia térmica (memoria o proyecto).
    SegunRite,
    /// Memoria con la documentación específica de A2L.
    MemoriaA2l,
    /// Proyecto frigorífico, aunque el RITE permita memoria.
    Proyecto,
}

/// Resultado de la clasificación.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clasificacion {
    pub regimen: Regimen,
    pub documento: DocumentoRsif,
    /// Firma de técnico titulado competente (justificación, análisis o proyecto).
    pub tecnico_competente: bool,
    /// Dirección técnica de la instalación.
    pub direccion_tecnica: bool,
    /// Empresa instaladora frigorista de nivel 2.
    pub empresa_nivel2: bool,
    /// Por qué (para mostrar al usuario).
    pub motivo: String,
    pub avisos: Vec<String>,
}

const LIM_SISTEMA: Decimal = Decimal::from_parts(30, 0, 0, false, 0);
const LIM_TOTAL: Decimal = Decimal::from_parts(100, 0, 0, false, 0);
const LIM_RITE: Decimal = Decimal::from_parts(70, 0, 0, false, 0);

/// Clasifica la instalación frigorífica.
pub fn clasificar(d: &DatosRsif) -> Clasificacion {
    let dentro = d.compresores_sistema_max_kw <= LIM_SISTEMA && d.compresores_total_kw <= LIM_TOTAL;
    let potencias = format!(
        "{} kW eléctricos en el mayor sistema y {} kW en total",
        d.compresores_sistema_max_kw.normalize(),
        d.compresores_total_kw.normalize()
    );
    let mut avisos = Vec::new();
    let nivel2 = |motivo: String, avisos: Vec<String>| Clasificacion {
        regimen: Regimen::Nivel2,
        documento: DocumentoRsif::Proyecto,
        tecnico_competente: true,
        direccion_tecnica: true,
        empresa_nivel2: true,
        motivo,
        avisos,
    };

    // IF-20: sistema indirecto cerrado con equipos compactos L2/L3.
    if d.indirecto_cerrado && d.compacto && matches!(d.grupo, Grupo::A2l | Grupo::L2 | Grupo::L3) {
        avisos.push(
            "IF-20: el mantenimiento del circuito primario lo hace una empresa frigorista de nivel 2 \
             o una empresa RITE habilitada según su apartado 4.2; el instalador no abre ni modifica el circuito."
                .into(),
        );
        avisos.push("Aportar el certificado de la instalación frigorífica (IF-10) con el expediente RITE.".into());
        return Clasificacion {
            regimen: Regimen::If20,
            documento: DocumentoRsif::SegunRite,
            tecnico_competente: false,
            direccion_tecnica: false,
            empresa_nivel2: false,
            motivo: "Sistema indirecto cerrado con equipos compactos de refrigerante L2/L3: condiciones de la IF-20; \
                     la documentación es la del tramo RITE."
                .into(),
            avisos,
        };
    }

    match d.grupo {
        Grupo::L1 => {
            if d.compacto && !d.camara_atmosfera_artificial {
                Clasificacion {
                    regimen: Regimen::Nivel1,
                    documento: DocumentoRsif::SegunRite,
                    tecnico_competente: false,
                    direccion_tecnica: false,
                    empresa_nivel2: false,
                    motivo: "Equipo compacto L1 con condensador incorporado: nivel 1 con cualquier potencia (art. 8)."
                        .into(),
                    avisos,
                }
            } else if dentro && !d.camara_atmosfera_artificial {
                Clasificacion {
                    regimen: Regimen::Nivel1,
                    documento: DocumentoRsif::SegunRite,
                    tecnico_competente: false,
                    direccion_tecnica: false,
                    empresa_nivel2: false,
                    motivo: format!("L1 con {potencias}: nivel 1."),
                    avisos,
                }
            } else {
                nivel2(
                    format!("L1 con {potencias} (más de 30 / 100 kW): nivel 2, proyecto y dirección técnica aunque la potencia térmica sea menor de 70 kW."),
                    avisos,
                )
            }
        }
        Grupo::A2l => {
            if !dentro || d.camara_atmosfera_artificial {
                return nivel2(
                    format!(
                        "A2L con {potencias}{}: fuera de la vía simplificada del art. 21.4, nivel 2 general con proyecto y dirección técnica.",
                        if d.camara_atmosfera_artificial { " y cámara de atmósfera artificial" } else { "" }
                    ),
                    avisos,
                );
            }
            avisos.push(
                "Comprobar la carga final (incluida la adicional por longitud de tubería) frente al local más \
                 desfavorable donde haya unidad o tubería."
                    .into(),
            );
            let (tecnico, nivel2_emp, como) = match d.justificacion_a2l {
                JustificacionCarga::Ordinaria => (false, false, "justificación ordinaria (IF-04, ap. 3): la firma el instalador habilitado"),
                JustificacionCarga::Apendice4 => (true, false, "medidas del apéndice 4 de la IF-04: la justificación la firma un técnico competente (consulta 74)"),
                JustificacionCarga::AnalisisRiesgos => (true, true, "análisis de riesgos del art. 21.3: técnico competente y empresa de nivel 2"),
            };
            Clasificacion {
                regimen: Regimen::A2lSimplificado,
                documento: DocumentoRsif::MemoriaA2l,
                tecnico_competente: tecnico,
                direccion_tecnica: false,
                empresa_nivel2: nivel2_emp,
                motivo: format!("A2L con {potencias}: memoria por la vía simplificada; {como}."),
                avisos,
            }
        }
        Grupo::L2 | Grupo::L3 => {
            avisos.push(
                "Antes de exigir proyecto, comprobar si aplica la exclusión por pequeña carga del artículo 2."
                    .into(),
            );
            nivel2(
                format!(
                    "Refrigerante {} fuera de la IF-20 (circuito construido o interconectado en obra): nivel 2 general, proyecto y dirección técnica aunque la potencia térmica sea menor de 70 kW.",
                    if d.grupo == Grupo::L3 { "L3" } else { "L2 no A2L" }
                ),
                avisos,
            )
        }
    }
}

/// Documento final combinando RITE (por potencia térmica) y RSIF.
pub fn proyecto_obligatorio(potencia_termica_kw: Decimal, c: &Clasificacion) -> bool {
    potencia_termica_kw > LIM_RITE || c.documento == DocumentoRsif::Proyecto
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(grupo: Grupo, compacto: bool, indirecto: bool, sis: Decimal, tot: Decimal) -> DatosRsif {
        DatosRsif {
            grupo,
            compacto,
            indirecto_cerrado: indirecto,
            compresores_sistema_max_kw: sis,
            compresores_total_kw: tot,
            ..DatosRsif::default()
        }
    }

    #[test]
    fn grupos_por_nombre() {
        assert_eq!(Grupo::de_refrigerante("R-32"), Some(Grupo::A2l));
        assert_eq!(Grupo::de_refrigerante("r290"), Some(Grupo::L3));
        assert_eq!(Grupo::de_refrigerante("R410A"), Some(Grupo::L1));
        assert_eq!(Grupo::de_refrigerante("R744"), Some(Grupo::L1));
        assert_eq!(Grupo::de_refrigerante("R999"), None);
    }

    #[test]
    fn r290_monobloc_indirecto_if20_segun_rite() {
        let c = clasificar(&d(Grupo::L3, true, true, dec!(5), dec!(5)));
        assert_eq!(c.regimen, Regimen::If20);
        assert_eq!(c.documento, DocumentoRsif::SegunRite);
        assert!(!c.tecnico_competente);
        assert!(!proyecto_obligatorio(dec!(15), &c));
    }

    #[test]
    fn r290_partido_en_obra_proyecto_aunque_menos_de_70() {
        let c = clasificar(&d(Grupo::L3, false, false, dec!(4), dec!(4)));
        assert_eq!(c.regimen, Regimen::Nivel2);
        assert!(proyecto_obligatorio(dec!(15), &c));
        assert!(c.direccion_tecnica);
    }

    #[test]
    fn l1_compacto_cualquier_potencia_nivel1() {
        let c = clasificar(&d(Grupo::L1, true, true, dec!(120), dec!(240)));
        assert_eq!(c.regimen, Regimen::Nivel1);
    }

    #[test]
    fn l1_partido_por_encima_de_30_kw_proyecto() {
        let c = clasificar(&d(Grupo::L1, false, false, dec!(35), dec!(35)));
        assert_eq!(c.regimen, Regimen::Nivel2);
        assert!(proyecto_obligatorio(dec!(60), &c));
    }

    #[test]
    fn split_r32_vivienda_memoria_firma_instalador() {
        let c = clasificar(&d(Grupo::A2l, false, false, dec!(1.6), dec!(1.6)));
        assert_eq!(c.regimen, Regimen::A2lSimplificado);
        assert_eq!(c.documento, DocumentoRsif::MemoriaA2l);
        assert!(!c.tecnico_competente);
        assert!(!proyecto_obligatorio(dec!(6), &c));
    }

    #[test]
    fn a2l_apendice4_y_analisis_de_riesgos() {
        let mut x = d(Grupo::A2l, false, false, dec!(14), dec!(14));
        x.justificacion_a2l = JustificacionCarga::Apendice4;
        let c = clasificar(&x);
        assert!(c.tecnico_competente && !c.empresa_nivel2);
        x.justificacion_a2l = JustificacionCarga::AnalisisRiesgos;
        let c = clasificar(&x);
        assert!(c.tecnico_competente && c.empresa_nivel2);
        assert_eq!(c.documento, DocumentoRsif::MemoriaA2l, "técnico no es proyecto");
    }

    #[test]
    fn vrf_r32_por_encima_de_30_kw_nivel2() {
        let c = clasificar(&d(Grupo::A2l, false, false, dec!(32), dec!(32)));
        assert_eq!(c.regimen, Regimen::Nivel2);
        assert!(proyecto_obligatorio(dec!(50), &c));
    }

    #[test]
    fn enfriadora_r32_compacta_indirecta_if20() {
        // Torre de Alcalá: EWYT390B-SLA2, R32, aire-agua
        let c = clasificar(&d(Grupo::A2l, true, true, dec!(60), dec!(120)));
        assert_eq!(c.regimen, Regimen::If20);
        assert!(proyecto_obligatorio(dec!(780), &c), "proyecto por el RITE (> 70 kW)");
    }
}
