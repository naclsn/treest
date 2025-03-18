local m = {}

m.commands = {
    cquit= function(arg) treest:quit(arg) end,
    echo= function(arg)
        local ok, err = load('return '..arg)
        if ok
          then
            _ = ok()
            treest:message(debug.pretty(_))
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
    for _, al in pairs({...})
      do m.commands[al] = m.commands[com]
    end
end
alias('cquit', 'cq')
alias('echo', 'ec')
alias('eval', 'ev', 'let', 'call', 'cal')
alias('help', 'h')
alias('quit', 'q')
alias('suspend', 'sus', 'stop', 'st')

local function search(q, flags)
    if not q then return end
    local found = treest:search(q, flags)
    if not found
      then treest:message("not found: "..treest:get_register('/'))
      else
        treest:message(nil)
        treest:jumpto(found)
    end
end

m.keys = {
    q= function() treest:quit() end,
    ZQ= function() treest:quit() end,
    ['<C-Z>']= function() treest:suspend() end,

    [':']= function()
        --- @type string
        local ans = treest:prompt(':', function() return {} end)
        if not ans then return end
        local st, ed = ans:find('%w+')
        if not st then return end
        local com = m.commands[ans:sub(st, ed)]
        if com
          then com(ans:sub((ed or #ans)+1))
          else treest:message("unknown command: "..ans:sub(st, ed))
        end
    end,

    ['/']= function() search(treest:prompt('/'), {'next', 'sat'}) end,
    ['?']= function() search(treest:prompt('/'), {'prev', 'sat'}) end,
    ['n']= function() search(treest:get_register('/'), {'next', 'sat'}) end,
    ['N']= function() search(treest:get_register('/'), {'prev', 'sat'}) end,

    ['<C-E>']= function() treest:view_down('line') end,
    ['<C-Y>']= function() treest:view_up('line') end,
    ['<BackwardWheel>']= function() treest:view_down('mouse') end,
    ['<ForwardWheel>']= function() treest:view_up('mouse') end,
    ['<C-D>']= function() treest:view_down('halfwin') end,
    ['<C-U>']= function() treest:view_up('halfwin') end,
    ['<C-F>']= function() treest:view_down('win') end,
    ['<C-B>']= function() treest:view_up('win') end,

    ['l']= function() treest:enter() end,
    ['h']= function() treest:leave() end,
    ['j']= function() treest:next() end,
    ['k']= function() treest:prev() end,

    ['L']= function() treest:fold(false) end,
    ['H']= function() treest:fold(true) end,

    ['<Space>']= function()
        treest:mark(not treest:marked())
        treest:next()
    end,

    ['<LeftMouse>']= function() treest:message(treest.mouse_event_pos) end,
}

m.init = function()
    for seq, cb in pairs(m.keys)
      do treest:map(seq, cb)
    end
    treest:unfold(--[[treest.cursor]])
end

return m
