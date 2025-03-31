local m = {}

---@param incompl string
---@param choices string[]
local function compgen(incompl, choices)
    if nil == incompl then return choices end

    local r, n = {}, 1
    for _, word in ipairs(choices)
      do if word:sub(1, #incompl) == incompl then r[n], n = word, n+1 end
    end
    return r
end

m.completions = {
    ---@param line string
    ---@param point number
    commands= function(line, point)
        local p = line:prompt_shell_like_split(point)
        local incompl = p.parts[p.in_part]
        local choices, n = {}, 1 for k, _ in pairs(m.commands) do choices[n], n = k, n+1 end
        return compgen(incompl, choices)
    end,

    ---@param line string
    ---@param point number
    files= function(line, point)
        local p = line:prompt_shell_like_split(point)
        local incompl = p.parts[p.in_part]
        local choices, n = {}, 1 for k, _ in os.list(incompl) do choices[n], n = k, n+1 end
        return compgen(incompl, choices)
    end,

    ---@param line string
    ---@param point number
    functions= function(line, point)
        local p = line:prompt_lua_tokens_split(point)
        local incompl = p.parts[p.in_part]

        local choices, nn = {}, 1
          do
            for name, val in pairs(_G)
              do
                choices[nn], nn = name, nn+1
                if 'table' == type(val)
                  then for subname in pairs(val) do choices[nn], nn = subname, nn+1 end
                end
            end
            for name in help('_treest'):gmatch('%S+')
              do choices[nn], nn = name, nn+1
            end
        end

        return compgen(incompl, choices)
    end,

    ---@type table<string, fun(line:string, point:integer): string[]>
    for_command= {},
}

m.commands = {
    help= function(arg) treest:message(help(arg) or ("no help for "..arg)) end,

    quit= function() treest:quit() end,
    cquit= function(arg) treest:quit(0 < #arg and arg or "!") end,
    suspend= function() treest:suspend() end,

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
}

local function request(req)
    return function()
        local text = treest:prompt(req..' ', m.completions.files)
        if not text then return end
        local res = treest:provider_request(req, nil, text)
        if res then treest:message(res) end
    end
end
m.commands.mk = request('mk')
m.commands.cp = request('cp')
m.commands.rm = request('rm')
m.commands.mv = request('mv')
m.commands.ch = request('ch')
m.commands.vi = request('vi')
m.commands.ex = request('ex')

local function alias(com, ...)
    for _, al in pairs{...} do m.commands[al] = m.commands[com] end
end
alias('cquit', 'cq')
alias('echo', 'ec')
alias('eval', 'ev', 'let', 'call', 'cal')
alias('help', 'h')
alias('quit', 'q')
alias('suspend', 'sus', 'stop', 'st')

local function complete(func, ...)
    for _, com in pairs{...} do m.completions.for_command[com] = func end
end
complete(m.completions.files, 'mk', 'cp', 'rm', 'ch', 'vi', 'ex') -- idk
complete(m.completions.functions, 'echo', 'ec', 'eval', 'ev', 'let', 'call', 'cal', 'help')

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
        local ans = treest:prompt(':', function(line, point)
            local com = line:match('(%w+)')
            if not com then return m.completions.commands(line, point) end
            local comp = m.completions.for_command[com]
            return comp and comp(line, point) or {}
        end)
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
        local ans = treest:prompt('!', m.completions.files)
        if not ans then return end
        local p = assert(io.popen(ans..' 2>&1', 'r'))
        treest:message(assert(p:read('*a')))
        p:close()
    end,

    ['/']= function() search(treest:prompt('/', function() end), {'next', 'sat'}) end,
    ['?']= function() search(treest:prompt('/', function() end), {'prev', 'sat'}) end,
    ['n']= function() search(treest:get_register('/'), {'next', 'sat'}) end,
    ['N']= function() search(treest:get_register('/'), {'prev', 'sat'}) end,

    ['<C-E>']= function() treest:view_down('line') end,
    ['<C-Y>']= function() treest:view_up('line') end,
    ['<C-D>']= function() treest:view_down('halfwin') end,
    ['<C-U>']= function() treest:view_up('halfwin') end,
    ['<C-F>']= function() treest:view_down('win') end,
    ['<C-B>']= function() treest:view_up('win') end,

    ['l']= function() treest:enter() end,
    ['h']= function() treest:leave() end,
    ['j']= function() treest:next('sat') end,
    ['k']= function() treest:prev('sat') end,

    ['L']= function() treest:unfold() end,
    ['H']= function() treest:fold() end,

    ['<Space>']= function()
        if treest:marked()
            then treest:unmark()
            else treest:mark()
        end
        treest:next('sat')
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

    ['<BackwardWheel>']= function()
        if false -- TODO: term_row - treest.mouse_event_pos.row < treest:get_option('msh')
            then treest:message_scroll_down('mouse')
            else treest:view_down('mouse')
        end
    end,
    ['<ForwardWheel>']= function()
        if false -- TODO: term_row - treest.mouse_event_pos.row < treest:get_option('msh')
            then treest:message_scroll_up('mouse')
            else treest:view_up('mouse')
        end
    end,

    ['[']= function() treest:message_scroll_up('line') end,
    [']']= function() treest:message_scroll_down('line') end,
    ['{']= function() treest:message_scroll_up('halfwin') end,
    ['}']= function() treest:message_scroll_down('halfwin') end,

    ['<C-L>']= function() treest:message({}) end,
}

m.init = function()
    for seq, cb in pairs(m.keys) do treest:map(seq, cb) end
    treest:unfold()
end

return m
