const defaults_keys = #{
    "q": Fn("quit"),
    "ZQ": Fn("quit"),
    "<C-Z>": Fn("suspend"),

    ":": |api| {
        let ans = api.prompt(":", |line, point| []) ?? return;
        let com = ans.split()[0];
        let arg = ans[com.len()+1..];
        switch com {
            "eval" | "ev" => api.source_text(arg),
            "echo" | "ec" => api.message(api.source_text(arg)),
            "help" | "h" => api.message(help(arg).reduce(|a, b| a+b)),
            "quit" | "q" => api.quit(),
            "cquit" | "cq" => api.cquit(if "" == arg { "cquit" } else { arg }),
            "suspend" | "sus" | "st" | "stop" => api.suspend(),
            _ => api.message("unknown command: " + com),
        }
    },

    "/": |api| {
        // no `let`: search_next already uses the "/" register
        api.prompt("/", |line, point| []) ?? return;
        let found = api.search_next() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.message(());
        api.jumpto(found);
    },
    "?": |api| {
        // no `let`: search_next already uses the "/" register
        api.prompt("/", |line, point| []) ?? return;
        let found = api.search_deep() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.message(());
        api.jumpto(found);
    },
    "n": |api| {
        let found = api.search_next() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.message(());
        api.jumpto(found);
    },
    "N": |api| {
        let found = api.search_prev() ?? {
            api.message("not found: " + api["/"]);
            return;
        };
        api.message(());
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

    "L": |api| api.fold(false),
    "H": |api| api.fold(true),

    "<Space>": |api| {
        api.mark(!api.marked());
        api.next();
    },

    "<LeftMouse>": |api| api.message(api.mouse_event_pos()),
};

//fn defaults_init(api) {
    api.map(defaults_keys);
    api.fold(false);
//}
//defaults_init(api);
