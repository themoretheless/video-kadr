use crate::jobs::JobKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    Ingest,
    Analysis,
    Export,
}

impl ResourceClass {
    pub const ALL: [Self; 3] = [Self::Ingest, Self::Analysis, Self::Export];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Analysis => "analysis",
            Self::Export => "export",
        }
    }

    pub const fn for_job(kind: JobKind) -> Self {
        match kind {
            JobKind::Import => Self::Ingest,
            JobKind::Proxy => Self::Analysis,
            JobKind::Edit | JobKind::Composition => Self::Export,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceClassLimits {
    pub ingest: usize,
    pub analysis: usize,
    pub export: usize,
}

impl ResourceClassLimits {
    pub const fn balanced(per_class: usize) -> Self {
        Self {
            ingest: per_class,
            analysis: per_class,
            export: per_class,
        }
    }

    pub const fn get(self, class: ResourceClass) -> usize {
        match class {
            ResourceClass::Ingest => self.ingest,
            ResourceClass::Analysis => self.analysis,
            ResourceClass::Export => self.export,
        }
    }
}
