
//api.register("hi", |api| api.hi("heeeeeeeere"));

api.register(":", |api| {
    let ans = api.prompt(":");
    //api.hi(ans.trim());
    if ans.starts_with("eval ") {
        api.source_text(ans[5..]);
    }
});

api.register("=", |api| api.source_text(api.prompt("=")));


api.register("<C-E>", Fn("view_down"));
api.register("<C-Y>", Fn("view_up"));
api.register("<BackwardWheel>", |api| api.view_down("mouse"));
api.register("<ForwardWheel>", |api| api.view_up("mouse"));
api.register("<C-D>", |api| api.view_down("halfwin"));
api.register("<C-U>", |api| api.view_up("halfwin"));
api.register("<C-F>", |api| api.view_down("win"));
api.register("<C-B>", |api| api.view_up("win"));

api.register("l", Fn("enter"));
api.register("h", Fn("leave"));
api.register("j", Fn("next"));
api.register("k", Fn("prev"));
api.register("q", Fn("quit"));


api.register("<LeftMouse>", |api| api.hi(api.mouse_event_pos()));
