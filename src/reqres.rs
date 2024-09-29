use std::mem::{self, MaybeUninit};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReqRes<Req, Res> {
    Request(Req),
    Response(Res),
}
use ReqRes::*;

#[allow(dead_code)]
impl<Req, Res> ReqRes<Req, Res> {
    pub fn new(req: Req) -> Self {
        Request(req)
    }

    #[inline]
    pub fn is_request(&self) -> bool {
        matches!(self, Request(_))
    }

    #[inline]
    pub fn is_response(&self) -> bool {
        matches!(self, Response(_))
    }

    #[inline]
    pub fn process(&mut self, f: impl FnOnce(Req) -> Res) {
        match self {
            Request(req) => {
                let crap = MaybeUninit::uninit();
                let res = f(mem::replace(req, unsafe { crap.assume_init() }));
                _ = MaybeUninit::new(mem::replace(self, Response(res)));
            }
            Response(_) => panic!("called `ReqRes::process()` on a `Response` value"),
        }
    }

    #[inline(always)]
    pub fn unwrap(self) -> Res {
        match self {
            Request(_) => panic!("called `ReqRes::unwrap()` on a `Request` value"),
            Response(res) => res,
        }
    }
}
