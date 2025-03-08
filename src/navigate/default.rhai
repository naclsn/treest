
//api.register("hi", |api| api.hi("heeeeeeeere"));

api.register(":", |api| {
    let ans = api.prompt(":");
    //api.hi(ans.trim());
    if ans.starts_with("eval ") {
        api.source(ans[5..]);
    }
});


api.register("l", Fn("enter"));
api.register("h", Fn("leave"));
api.register("j", Fn("next"));
api.register("k", Fn("prev"));
api.register("q", Fn("quit"));
