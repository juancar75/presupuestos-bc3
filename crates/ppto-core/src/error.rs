// SPDX-License-Identifier: GPL-3.0-or-later
use thiserror::Error;

/// Errores del motor económico. Los mensajes van en castellano porque se
/// muestran directamente al presupuestador.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ErrorMotor {
    #[error("el concepto «{0}» no existe en el presupuesto")]
    ConceptoInexistente(String),

    #[error("referencia circular en la descomposición: {}", .0.join(" → "))]
    ReferenciaCircular(Vec<String>),

    #[error("el código «{0}» está duplicado")]
    CodigoDuplicado(String),

    #[error("fórmula de medición no válida «{formula}»: {motivo}")]
    FormulaInvalida { formula: String, motivo: String },

    #[error("división por cero en la fórmula «{0}»")]
    DivisionPorCero(String),

    #[error("la partida «{partida}» no contiene el recurso «{recurso}» en su descomposición directa")]
    RecursoNoEnPartida { partida: String, recurso: String },

    #[error(
        "doble sustitución: el recurso «{recurso}» de la partida «{partida}» ya lo cubre el paquete «{paquete_previo}»"
    )]
    DobleSustitucion {
        partida: String,
        recurso: String,
        paquete_previo: String,
    },

    #[error("la partida «{0}» no tiene medición en el presupuesto")]
    PartidaSinMedicion(String),

    #[error("valor no válido: {0}")]
    ValorInvalido(String),
}
