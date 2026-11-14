//! Checking the user's password with PAM, as a screen locker does: only `pam_authenticate`, with
//! the `login` service. It blocks, for seconds after a wrong password, so call it off the UI thread.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

const SERVICE: &str = "login";

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_AUTH_ERR: c_int = 7;
const PAM_MAXTRIES: c_int = 11;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

type Converse = extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int;

#[repr(C)]
struct Conversation {
    converse: Converse,
    data: *mut c_void,
}

/// What the conversation answers with, and what PAM tells on the way, like that faillock has
/// locked the account.
struct Answers {
    user: CString,
    password: CString,
    told: std::cell::RefCell<Vec<String>>,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const Conversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
    fn pam_strerror(handle: *mut c_void, error: c_int) -> *const c_char;
}

unsafe extern "C" {
    fn calloc(count: usize, size: usize) -> *mut c_void;
    fn strdup(s: *const c_char) -> *mut c_char;
    fn free(p: *mut c_void);
}

/// Answers PAM's prompts: the password where it isn't shown, the user where it is. The responses
/// are PAM's to free.
extern "C" fn converse(
    count: c_int,
    messages: *mut *const Message,
    responses: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || messages.is_null() || responses.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let answers = unsafe { &*(data as *const Answers) };
    let count = count as usize;
    let out = unsafe { calloc(count, size_of::<Response>()) } as *mut Response;
    if out.is_null() {
        return PAM_BUF_ERR;
    }
    for i in 0..count {
        // Linux-PAM passes an array of pointers to messages.
        let message = unsafe { *messages.add(i) };
        if message.is_null() {
            continue;
        }
        let style = unsafe { (*message).style };
        let answer = match style {
            PAM_PROMPT_ECHO_OFF => Some(&answers.password),
            PAM_PROMPT_ECHO_ON => Some(&answers.user),
            // Information and errors need no answer.
            _ => None,
        };
        let text = unsafe { (*message).text };
        if matches!(style, PAM_ERROR_MSG | PAM_TEXT_INFO) && !text.is_null() {
            let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
            let text = text.trim();
            if !text.is_empty() {
                answers.told.borrow_mut().push(text.to_owned());
            }
        }
        if let Some(answer) = answer {
            let text = unsafe { strdup(answer.as_ptr()) };
            if text.is_null() {
                for j in 0..i {
                    let r = unsafe { &mut *out.add(j) };
                    if !r.text.is_null() {
                        unsafe { free(r.text as *mut c_void) };
                    }
                }
                unsafe { free(out as *mut c_void) };
                return PAM_BUF_ERR;
            }
            unsafe { (*out.add(i)).text = text };
        }
    }
    unsafe { *responses = out };
    PAM_SUCCESS
}

/// Why it didn't let the user in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    WrongPassword,
    /// The password was refused, and PAM said why, like that the account is locked for a while.
    Told(String),
    /// PAM itself said no, or couldn't be asked, in its words.
    Other(String),
}

/// Whether `password` is the password of `user`.
pub fn authenticate(user: &str, password: String) -> Result<(), Refused> {
    let user = CString::new(user).map_err(|_| Refused::Other("bad user name".into()))?;
    let mut bytes = password.into_bytes();
    let password = CString::new(bytes.clone()).map_err(|_| Refused::WrongPassword);
    // The copy in `bytes` is gone with this.
    bytes.iter_mut().for_each(|b| *b = 0);
    let answers = Answers {
        user,
        password: password?,
        told: Default::default(),
    };
    let service = CString::new(SERVICE).expect("no nul");
    let conversation = Conversation {
        converse,
        data: &answers as *const Answers as *mut c_void,
    };
    let mut handle: *mut c_void = ptr::null_mut();
    let started = unsafe {
        pam_start(
            service.as_ptr(),
            answers.user.as_ptr(),
            &conversation,
            &mut handle,
        )
    };
    let result = if started != PAM_SUCCESS || handle.is_null() {
        Err(Refused::Other("PAM couldn't start".into()))
    } else {
        let code = unsafe { pam_authenticate(handle, 0) };
        let result = match code {
            PAM_SUCCESS => Ok(()),
            PAM_AUTH_ERR | PAM_MAXTRIES => {
                let told = answers.told.borrow();
                Err(if told.is_empty() {
                    Refused::WrongPassword
                } else {
                    // Like faillock's two lines: locked, and for how long.
                    Refused::Told(told.join(" "))
                })
            }
            other => {
                let text = unsafe { pam_strerror(handle, other) };
                Err(Refused::Other(if text.is_null() {
                    format!("PAM error {other}")
                } else {
                    unsafe { CStr::from_ptr(text) }
                        .to_string_lossy()
                        .into_owned()
                }))
            }
        };
        unsafe { pam_end(handle, code) };
        result
    };
    // Overwrites the password before it is freed.
    let mut secret = answers.password.into_bytes_with_nul();
    secret.iter_mut().for_each(|b| *b = 0);
    result
}
