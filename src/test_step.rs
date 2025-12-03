use crate::err_msg::WithErrMsg;
use serde::{Deserialize, Serialize};

// defaults to false for backwards compatibility
pub fn ret_false() -> bool {
    false
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TestStep {
    #[serde(default = "ret_false")]
    pub is_stepless_comment: bool,
    #[serde(default = "ret_false")]
    pub is_new_section: bool,
    pub instructions: String,
    pub expected_results: String,
    pub ac: String,
}

impl TestStep {
    pub fn new(is_stepless_comment: bool, is_new_section: bool) -> TestStep {
        TestStep {
            is_stepless_comment,
            is_new_section,
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

        let comment_section_str = "# comment section";
        let new_section_str = "# new section";
        let instructions_str = "# instructions";

        let maybe_new_section = lower.find(new_section_str).map(|idx| (idx, true, false));
        let maybe_comment_section = lower
            .find(comment_section_str)
            .map(|idx| (idx, false, true));

        let (idx, is_new_section, is_stepless_comment) = match maybe_new_section {
            Some(t) => t,
            None => match maybe_comment_section {
                Some(t) => t,
                None => (
                    lower
                        .find(instructions_str)
                        .with_err_msg(&"Failed to find instructions")?,
                    false,
                    false,
                ),
            },
        };

        let offset = if is_new_section {
            new_section_str.len()
        } else if is_stepless_comment {
            comment_section_str.len()
        } else {
            instructions_str.len()
        };

        let input = input.split_at(idx + offset).1;

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
            is_stepless_comment,
            is_new_section,
            instructions,
            expected_results,
            ac,
        };

        Ok(data)
    }

    pub fn gen_markdown(&self) -> String {
        let pre_str = if self.is_new_section {
            "# New Section"
        } else if self.is_stepless_comment {
            "# Comment Section"
        } else {
            "# Instructions"
        };
        format!(
            "{}\n{}\n\n# Expected Results\n{}\n\n# AC\n{}",
            pre_str,
            self.instructions(),
            self.expected_results(),
            self.ac()
        )
    }
}
