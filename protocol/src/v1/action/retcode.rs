use lazy_static::lazy_static;
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct Retcode {
    #[serde(rename = "retcode")]
    ret_code: i32,
    message: String,
}

impl Retcode {
    pub fn with_message(&self, msg: &str) -> Retcode {
        Retcode {
            ret_code: self.ret_code,
            message: format!("{}: {}", self.message, msg),
        }
    }
}

lazy_static! {
    pub static ref OK: Retcode = Retcode {
        ret_code: 0,
        message: "ok".to_string(),
    };
    // Request Errors (10000-19999)
    pub static ref REQUEST_ERROR: Retcode = Retcode {
        ret_code: 10000,
        message: "Request Error".to_string(),
    };
    pub static ref BAD_REQUEST: Retcode = Retcode {
        ret_code: 10001,
        message: "Bad Request".to_string(),
    };
    pub static ref UNKNOWN_ACTION: Retcode = Retcode {
        ret_code: 10002,
        message: "Unknown Action".to_string(),
    };
    pub static ref PERMISSION_DENIED: Retcode = Retcode {
        ret_code: 10003,
        message: "Permission Denied".to_string(),
    };
    pub static ref ACTION_UNAVAILABLE: Retcode = Retcode {
        ret_code: 10004,
        message: "Action Unavailable".to_string(),
    };
    pub static ref RATE_LIMIT_EXCEEDED: Retcode = Retcode {
        ret_code: 10005,
        message: "Rate Limit Exceeded".to_string(),
    };
    pub static ref PARAM_ERROR: Retcode = Retcode {
        ret_code: 10006,
        message: "Param Error".to_string(),
    };

    // Unexpected Error
    pub static ref UNEXPECTED_ERROR: Retcode = Retcode {
        ret_code: 20001,
        message: "Unexpected Error".to_string(),
    };

    // File Errors (20000-29999)
    pub static ref FILE_ERROR: Retcode = Retcode {
        ret_code: 21000,
        message: "File Error".to_string(),
    };
    pub static ref FILE_NOT_FOUND: Retcode = Retcode {
        ret_code: 21001,
        message: "File Not Found".to_string(),
    };
    pub static ref FILE_ALREADY_EXISTS: Retcode = Retcode {
        ret_code: 21002,
        message: "File Already Exists".to_string(),
    };
    pub static ref FILE_IN_USE: Retcode = Retcode {
        ret_code: 21003,
        message: "File In Use".to_string(),
    };
    pub static ref ITS_A_DIRECTORY: Retcode = Retcode {
        ret_code: 21004,
        message: "It's A Directory".to_string(),
    };
    pub static ref ITS_A_FILE: Retcode = Retcode {
        ret_code: 21005,
        message: "It's A File".to_string(),
    };
    pub static ref FILE_ACCESS_DENIED: Retcode = Retcode {
        ret_code: 21006,
        message: "File Access Denied".to_string(),
    };
    pub static ref DISK_FULL: Retcode = Retcode {
        ret_code: 21007,
        message: "Disk Full".to_string(),
    };

    // Upload/Download Errors
    pub static ref UPLOAD_DOWNLOAD_ERROR: Retcode = Retcode {
        ret_code: 21100,
        message: "Upload/Download Error".to_string(),
    };
    pub static ref ALREADY_UPLOADING_DOWNLOADING: Retcode = Retcode {
        ret_code: 21101,
        message: "Already Uploading/Downloading".to_string(),
    };
    pub static ref NOT_UPLOADING_DOWNLOADING: Retcode = Retcode {
        ret_code: 21102,
        message: "Not Uploading/Downloading".to_string(),
    };
    pub static ref FILE_TOO_BIG: Retcode = Retcode {
        ret_code: 21103,
        message: "File Too Big".to_string(),
    };

    // Instance Errors (30000-39999)
    pub static ref INSTANCE_ERROR: Retcode = Retcode {
        ret_code: 30000,
        message: "Instance Error".to_string(),
    };
    pub static ref INSTANCE_NOT_FOUND: Retcode = Retcode {
        ret_code: 30001,
        message: "Instance Not Found".to_string(),
    };
    pub static ref INSTANCE_ALREADY_EXISTS: Retcode = Retcode {
        ret_code: 30002,
        message: "Instance Already Exists".to_string(),
    };
    pub static ref BAD_INSTANCE_STATE: Retcode = Retcode {
        ret_code: 30003,
        message: "Bad Instance State".to_string(),
    };
    pub static ref BAD_INSTANCE_TYPE: Retcode = Retcode {
        ret_code: 30004,
        message: "Bad Instance Type".to_string(),
    };

    // Instance Action Errors
    pub static ref INSTANCE_ACTION_ERROR: Retcode = Retcode {
        ret_code: 31001,
        message: "Instance Action Error".to_string(),
    };
    pub static ref INSTALLATION_ERROR: Retcode = Retcode {
        ret_code: 31002,
        message: "Installation Error".to_string(),
    };
    pub static ref PROCESS_ERROR: Retcode = Retcode {
        ret_code: 31003,
        message: "Process Error".to_string(),
    };
}
