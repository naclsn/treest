api.register(#{
    ":": |api| {
        let ans = api.prompt(":", |line, point| []) ?? return;
        //api.hi(ans.trim());
        if ans.starts_with("eval ") {
            api.source_text(ans[5..]);
        }
    },

    "=": |api| {
        let s = api.prompt("=", |line, point| []) ?? return;
        api.message(api.source_text(s));
    },

    "/": |api| {
        // no `let`: search_next already uses the "/" register
        api.prompt("/", |line, point| []) ?? return;
        let found = api.search_next() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.jumpto(found);
    },
    "?": |api| {
        // no `let`: search_next already uses the "/" register
        api.prompt("/", |line, point| []) ?? return;
        let found = api.search_deep() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.jumpto(found);
    },
    "n": |api| {
        let found = api.search_next() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.jumpto(found);
    },
    "N": |api| {
        let found = api.search_prev() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.jumpto(found);
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
        api.mark(!api.marked());
        api.next();
    },

    "<LeftMouse>": |api| api.message(api.mouse_event_pos()),
});
