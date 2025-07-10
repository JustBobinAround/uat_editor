use crate::err_msg::WithErrMsg;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TestStep {
    pub instructions: String,
    pub expected_results: String,
    pub ac: String,
}

impl TestStep {
    pub fn new() -> TestStep {
        TestStep {
            instructions: String::new(),
            expected_results: String::new(),
            ac: String::new(),
        }
    }

    pub fn ref_array(&self) -> [String; 3] {
        [self.instructions(), self.expected_results(), self.ac()]
    }

    pub fn instructions(&self) -> String {
        self.instructions.trim().to_string()
    }

    pub fn expected_results(&self) -> String {
        self.expected_results.trim().to_string()
    }

    pub fn ac(&self) -> String {
        self.ac.trim().to_string()
    }

    pub fn parse_markdown(input: &String) -> Result<TestStep, String> {
        let lower = input.to_lowercase();

        let instructions;
        let expected_results;
        let ac;

        let idx = lower
            .find("# instructions")
            .with_err_msg(&"Could not find instruction section")?;

        let input = input.split_at(idx + 15).1;
        let lower = input.to_lowercase();

        let idx = lower
            .find("# expected results")
            .with_err_msg(&"Could not find expected results section")?;

        let split = input.split_at(idx);
        let splitb = input.split_at(idx + 19);
        instructions = split.0.to_string();
        let input = splitb.1;
        let lower = input.to_lowercase();

        let idx = lower
            .find("# ac")
            .with_err_msg(&"Could not find ac section")?;

        let split = input.split_at(idx);
        let splitb = input.split_at(idx + 5);
        expected_results = split.0.to_string();
        ac = splitb.1.to_string();

        let data = TestStep {
            instructions,
            expected_results,
            ac,
        };

        Ok(data)
    }

    pub fn gen_markdown(&self) -> String {
        format!(
            "# Instructions\n{}\n\n# Expected Results\n{}\n\n# AC\n{}",
            self.instructions(),
            self.expected_results(),
            self.ac()
        )
    }
}
