#[derive(Debug, Clone, PartialEq)]
pub enum WfResult<T, E = CommonError> {
    Ok(T),
    Err(E),
}

impl<T, E> WfResult<T, E> {
    pub fn is_ok(&self) -> bool {
        matches!(self, WfResult::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, WfResult::Err(_))
    }

    pub fn ok(self) -> Option<T> {
        match self {
            WfResult::Ok(v) => Some(v),
            WfResult::Err(_) => None,
        }
    }

    pub fn err(self) -> Option<E> {
        match self {
            WfResult::Ok(_) => None,
            WfResult::Err(e) => Some(e),
        }
    }

    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            WfResult::Ok(v) => v,
            WfResult::Err(e) => panic!("called unwrap on an Err: {:?}", e),
        }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> WfResult<U, E> {
        match self {
            WfResult::Ok(v) => WfResult::Ok(f(v)),
            WfResult::Err(e) => WfResult::Err(e),
        }
    }

    pub fn map_err<F, G: FnOnce(E) -> F>(self, f: G) -> WfResult<T, F> {
        match self {
            WfResult::Ok(v) => WfResult::Ok(v),
            WfResult::Err(e) => WfResult::Err(f(e)),
        }
    }

    pub fn and_then<U, F: FnOnce(T) -> WfResult<U, E>>(self, f: F) -> WfResult<U, E> {
        match self {
            WfResult::Ok(v) => f(v),
            WfResult::Err(e) => WfResult::Err(e),
        }
    }
}

impl<T: fmt::Debug, E: fmt::Debug> WfResult<T, E> {
    pub fn expect(self, msg: &str) -> T {
        match self {
            WfResult::Ok(v) => v,
            WfResult::Err(e) => panic!("{}: {:?}", msg, e),
        }
    }
}

impl<E> WfResult<(), E> {
    pub fn ok_or(err: E) -> Self {
        WfResult::Err(err)
    }
}

pub fn ok<T, E>(value: T) -> WfResult<T, E> {
    WfResult::Ok(value)
}

pub fn err<T, E>(error: E) -> WfResult<T, E> {
    WfResult::Err(error)
}

impl<T, E> AsRef<E> for WfResult<T, E>
where
    E: std::fmt::Debug,
{
    fn as_ref(&self) -> &E {
        match self {
            WfResult::Ok(_) => panic!("called as_ref on Ok"),
            WfResult::Err(e) => e,
        }
    }
}
