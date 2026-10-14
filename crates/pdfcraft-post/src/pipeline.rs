//! Ordered post-processing steps on one PDF byte buffer.

use crate::combo::StampAndEncrypt;
use crate::encryption::Encrypt;
use crate::error::Error;
use crate::metadata::SetMetadata;

/// One step in a [`Pipeline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Apply [`SetMetadata`].
    Metadata(SetMetadata),
    /// Apply [`Encrypt`].
    Encrypt(Encrypt),
    /// Apply [`StampAndEncrypt`].
    StampAndEncrypt(StampAndEncrypt),
}

impl Step {
    fn apply(&self, pdf: &[u8]) -> Result<Vec<u8>, Error> {
        match self {
            Self::Metadata(step) => step.apply(pdf),
            Self::Encrypt(step) => step.apply(pdf),
            Self::StampAndEncrypt(step) => step.apply(pdf),
        }
    }
}

/// Ordered list of post-processors.
///
/// An empty pipeline returns the input bytes unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pipeline {
    steps: Vec<Step>,
}

impl Pipeline {
    /// Build a pipeline from steps in application order.
    #[must_use]
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }

    /// Steps in this pipeline.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Run every step in order.
    pub fn run(&self, pdf: &[u8]) -> Result<Vec<u8>, Error> {
        if self.steps.is_empty() {
            return Ok(pdf.to_vec());
        }
        let mut current = pdf.to_vec();
        for step in &self.steps {
            current = step.apply(&current)?;
        }
        Ok(current)
    }
}
