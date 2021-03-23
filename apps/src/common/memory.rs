pub type SocketId = i32;

pub const SOCKET_ID_ANY: SocketId = -1;

pub trait AsRef<'a, T: 'a> {
    fn as_ref(self) -> Option<&'a T>;
}

pub trait AsMutRef<'a, T: 'a> {
    fn as_mut_ref(self) -> Option<&'a mut T>;
}

impl<'a, T: 'a> AsRef<'a, T> for *const T {
    fn as_ref(self) -> Option<&'a T> {
        if self.is_null() {
            None
        } else {
            Some(unsafe { &*self })
        }
    }
}

impl<'a, T: 'a> AsMutRef<'a, T> for *mut T {
    fn as_mut_ref(self) -> Option<&'a mut T> {
        if self.is_null() {
            None
        } else {
            Some(unsafe { &mut *self })
        }
    }
}

impl<'a, T: 'a> AsRef<'a, T> for Option<*const T> {
    fn as_ref(self) -> Option<&'a T> {
        self.map(|p| unsafe { &*p })
    }
}

impl<'a, T: 'a> AsMutRef<'a, T> for Option<*mut T> {
    fn as_mut_ref(self) -> Option<&'a mut T> {
        self.map(|p| unsafe { &mut *p })
    }
}

impl<'a, T: 'a, E> AsRef<'a, T> for Result<*const T, E> {
    fn as_ref(self) -> Option<&'a T> {
        self.ok().map(|p| unsafe { &*p })
    }
}

impl<'a, T: 'a, E> AsMutRef<'a, T> for Result<*mut T, E> {
    fn as_mut_ref(self) -> Option<&'a mut T> {
        self.ok().map(|p| unsafe { &mut *p })
    }
}
