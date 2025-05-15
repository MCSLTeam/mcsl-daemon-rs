use std::collections::HashMap;
use strfmt::FmtError;
use thiserror::Error;

// 自定义错误类型
#[derive(Error, Debug)]
pub enum PlaceHolderError {
    #[error("formatting error: {0}")]
    FormatError(#[from] FmtError),
}

// 定义 PlaceHolder trait
pub trait PlaceHolderRender {
    /// 格式化字符串，将占位符替换为实际值
    ///
    /// # Arguments
    /// * `vars` - 包含占位符键值对的 HashMap
    ///
    /// # Returns
    /// * `Ok(String)` - 替换后的字符串
    /// * `Err(PlaceHolderError)` - 替换过程中发生的错误
    fn format(&self, vars: &HashMap<String, String>) -> Result<String, PlaceHolderError>;
}

// 为 String 实现 PlaceHolder trait
impl PlaceHolderRender for String {
    fn format(&self, vars: &HashMap<String, String>) -> Result<String, PlaceHolderError> {
        strfmt::strfmt(self, vars).map_err(PlaceHolderError::FormatError)
    }
}

// 为 &str 实现 PlaceHolder trait
impl PlaceHolderRender for &str {
    fn format(&self, vars: &HashMap<String, String>) -> Result<String, PlaceHolderError> {
        strfmt::strfmt(self, vars).map_err(PlaceHolderError::FormatError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_format_string_success() {
        let template = "{Root}/{LibName}/bin".to_string();
        let mut vars = HashMap::new();
        vars.insert("Root".to_string(), "/home/.mcsl".to_string());
        vars.insert("LibName".to_string(), "lib".to_string());

        let result = template.format(&vars).unwrap();
        assert_eq!(result, "/home/.mcsl/lib/bin");
    }

    #[test]
    fn test_format_str_success() {
        let template = "{Root}/{LibName}/bin";
        let mut vars = HashMap::new();
        vars.insert("Root".to_string(), "/home/.mcsl".to_string());
        vars.insert("LibName".to_string(), "lib".to_string());

        let result = template.format(&vars).unwrap();
        assert_eq!(result, "/home/.mcsl/lib/bin");
    }

    #[test]
    fn test_format_missing_var() {
        let template = "{Root}/{Unknown}/bin".to_string();
        let mut vars = HashMap::new();
        vars.insert("Root".to_string(), "/home/.mcsl".to_string());

        let result = template.format(&vars);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, PlaceHolderError::FormatError(_)));
        }
    }
}
