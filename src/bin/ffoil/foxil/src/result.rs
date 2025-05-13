use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct XfoilResult {
    alpha: Vec<f64>,

    #[serde(rename = "CL")]
    cl: Vec<f64>,
    #[serde(rename = "CD")]
    cd: Vec<f64>,
    #[serde(rename = "CDp")]
    cd_p: Vec<f64>,
    #[serde(rename = "CM")]
    cm: Vec<f64>,
    #[serde(rename = "Top_Xtr")]
    top_xtr: Vec<f64>,
    #[serde(rename = "Bot_Xtr")]
    bot_xtr: Vec<f64>,
}

impl XfoilResult {
    pub fn get_analysis_result(&self, aoa: f64) -> AnalysisResult {
        if let Some(idx) = self.alpha.iter().position(|&x| x == aoa) {
            let cl = self.cl.get(idx).copied().expect("cl not found!");
            let cd = self.cd.get(idx).copied().expect("cd not found!");
            AnalysisResult::valid_result(aoa, cl, cd)
        } else {
            AnalysisResult::default()
        }
    }
    pub fn export(&self) -> Vec<AnalysisResult> {
        self.alpha
            .iter()
            .map(|&aoa| self.get_analysis_result(aoa))
            .collect()
    }

    pub fn to_csv(self, path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let mut wtr = csv::Writer::from_path(path)?;

        // Write headers matching the XfoilResult fields
        wtr.write_record(["alpha", "CL", "CD", "CDp", "CM", "Top_Xtr", "Bot_Xtr"])?;

        // Iterate over the data points.
        // Assumes all Vec<f64> fields in XfoilResult have the same length,
        // corresponding to the number of alpha values.
        // This should be guaranteed by the parsing logic that creates XfoilResult.
        if !self.alpha.is_empty() {
            for i in 0..self.alpha.len() {
                // It's generally safe to use direct indexing if the invariant holds.
                // Otherwise, .get(i).copied().unwrap_or_default() could be used for robustness,
                // but that would mask data integrity issues.
                let record = [
                    self.alpha[i].to_string(),
                    self.cl[i].to_string(),
                    self.cd[i].to_string(),
                    self.cd_p[i].to_string(),
                    self.cm[i].to_string(),
                    self.top_xtr[i].to_string(),
                    self.bot_xtr[i].to_string(),
                ];
                wtr.write_record(&record)?;
            }
        }

        wtr.flush()?; // Ensure all data is written to the underlying writer.
        Ok(self)
    }
}

#[derive(Default)]
pub struct AnalysisResult {
    pub aoa: f64,
    pub cl: f64,
    pub cd: f64,
    pub ld_ratio: f64,
}

impl AnalysisResult {
    fn valid_result(aoa: f64, cl: f64, cd: f64) -> Self {
        let ld_ratio = if cd.abs() < 1e-9 { 0.0 } else { cl / cd }; // Avoid division by zero
        AnalysisResult {
            aoa,
            cl,
            cd,
            ld_ratio,
        }
    }
}
