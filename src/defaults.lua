local m = {}

m.commands = {
    cquit= function(arg) treest:quit(arg) end,
    echo= function(arg, bang)
        local ok, err = load('return '..arg)
        if ok
          then
            _ = ok()
            if bang and nil == _ then return end
            treest:message((bang and debug.pretty or tostring)(_))
            return
        end
        treest:message(err)
    end,
    eval= function(arg)
        local ok, err = load(arg)
        if ok then ok() return end
        treest:message(err)
    end,
    help= function(arg) treest:message(help(arg) or ("no help for "..arg)) end,
    quit= function() treest:quit() end,
    suspend= function() treest:suspend() end,
}

local function alias(com, ...)
    for _, al in pairs({...}) do m.commands[al] = m.commands[com] end
end
alias('cquit', 'cq')
alias('echo', 'ec')
alias('eval', 'ev', 'let', 'call', 'cal')
alias('help', 'h')
alias('quit', 'q')
alias('suspend', 'sus', 'stop', 'st')

local function search(q, flags)
    if not q then return end
    local found = treest:search_level(q, flags)
    if not found
      then treest:message("not found: "..treest:get_register('/'))
      else
        treest:message(nil)
        treest:set_cursor(found)
        return found
    end
end

m.keys = {
    q= function() treest:quit() end,
    ZQ= function() treest:quit() end,
    ['<C-Z>']= function() treest:suspend() end,

    [':']= function()
        local ans = treest:prompt(':', function() return {} end)
        if not ans then return end
        local com, bang, arg = ans:match('(%w+)(!?)%s*(.*)')
        if not com then return end
        local fn = m.commands[com]
        if fn
            then fn(arg, '!' == bang)
            else treest:message("unknown command: "..com)
        end
    end,
    ['!']= function()
        local ans = treest:prompt('!', function() return {} end)
        if not ans then return end
        local f = assert(io.popen(ans..' 2>&1', 'r'))
        local out = assert(f:read('*a'))
        f:close()
        treest:message(out)
    end,

    ['/']= function() search(treest:prompt('/', function() end), {'next', 'sat'}) end,
    ['?']= function() search(treest:prompt('/', function() end), {'prev', 'sat'}) end,
    ['n']= function() search(treest:get_register('/'), {'next', 'sat'}) end,
    ['N']= function() search(treest:get_register('/'), {'prev', 'sat'}) end,

    ['<C-E>']= function() treest:view_down({'line'}) end,
    ['<C-Y>']= function() treest:view_up({'line'}) end,
    ['<BackwardWheel>']= function() treest:view_down({'mouse'}) end,
    ['<ForwardWheel>']= function() treest:view_up({'mouse'}) end,
    ['<C-D>']= function() treest:view_down({'halfwin'}) end,
    ['<C-U>']= function() treest:view_up({'halfwin'}) end,
    ['<C-F>']= function() treest:view_down({'win'}) end,
    ['<C-B>']= function() treest:view_up({'win'}) end,

    ['l']= function() treest:enter() end,
    ['h']= function() treest:leave() end,
    ['j']= function() treest:next({'sat'}) end,
    ['k']= function() treest:prev({'sat'}) end,

    ['L']= function() treest:unfold() end,
    ['H']= function() treest:fold() end,

    ['<Space>']= function()
        if treest:marked()
            then treest:unmark()
            else treest:mark()
        end
        treest:next({'sat'})
    end,

    ['<LeftMouse>']= function()
        local node = treest:node_at_line(treest.mouse_event_pos.row)
        if not node then return end
        treest:set_cursor(node.path)
    end,
    ['<RightMouse>']= function()
        local node = treest:node_at_line(treest.mouse_event_pos.row)
        if not node then return end
        if treest:folded(node.path)
            then treest:unfold(node.path)
            else treest:fold(node.path)
        end
    end,
}

m.init = function()
    for seq, cb in pairs(m.keys) do treest:map(seq, cb) end
    treest:unfold()
end

return m
