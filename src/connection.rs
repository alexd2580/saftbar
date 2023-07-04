use std::{ops::Deref, ptr::null};

pub struct Connection(xcb::Connection);

impl Deref for Connection {
    type Target = xcb::Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Connection {
    pub fn new() -> Self {
        let display = unsafe { x11::xlib::XOpenDisplay(null()) };

        let extensions = [xcb::Extension::RandR];
        let connection =
            unsafe { xcb::Connection::from_xlib_display_and_extensions(display, &extensions, &[]) };

        Self(connection)
    }

    /// Execute a request and wait for the reply. Check for request completion.
    pub fn exec<Request>(
        &self,
        request: &Request,
    ) -> <<Request as xcb::Request>::Cookie as xcb::CookieWithReplyChecked>::Reply
    where
        Request: xcb::Request,
        <Request as xcb::Request>::Cookie: xcb::CookieWithReplyChecked,
    {
        let cookie = self.send_request(request);
        self.wait_for_reply(cookie).unwrap()
    }

    /// Execute a request that has no reply. Check for request completion.
    pub fn exec_<Request>(&self, request: &Request)
    where
        Request: xcb::RequestWithoutReply + std::fmt::Debug,
    {
        if let Err(err) = self.send_and_check_request(request) {
            dbg!(&request);
            panic!("{}", err);
        };
    }
}
