api.register(#{
    ":": |api| {
        let ans = api.prompt(":", |line, point| []);
        if () == ans { return; }
        //api.hi(ans.trim());
        if ans.starts_with("eval ") {
            api.source_text(ans[5..]);
        }
    },

    "=": |api| {
        let s = api.prompt("=", |line, point| []);
        if () == s { return; }
        api.hi(api.source_text(s));
    },


    "<C-E>": Fn("view_down"),
    "<C-Y>": Fn("view_up"),
    "<BackwardWheel>": |api| api.view_down("mouse"),
    "<ForwardWheel>": |api| api.view_up("mouse"),
    "<C-D>": |api| api.view_down("halfwin"),
    "<C-U>": |api| api.view_up("halfwin"),
    "<C-F>": |api| api.view_down("win"),
    "<C-B>": |api| api.view_up("win"),

    "l": Fn("enter"),
    "h": Fn("leave"),
    "j": Fn("next"),
    "k": Fn("prev"),
    "q": Fn("quit"),

    "L": |api| api.fold(false),
    "H": |api| api.fold(true),

    "<Space>": |api| {
        api.mark(true);
        api.next();
    },

    "<LeftMouse>": |api| api.hi(api.mouse_event_pos()),
});
