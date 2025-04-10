local m = {}

---@param incompl string
---@param choices string[]
local function compgen(incompl, choices)
    if '' == incompl then return choices end
    local r, n = {}, 1
    for _, word in ipairs(choices)
    do
        if word:sub(1, #incompl) == incompl then r[n], n = word, n + 1 end
    end
    return r
end

m.completions = {
    ---@param line string
    ---@param point number
    commands = function(line, point)
        local p = line:prompt_shell_like_split(point)
        local incompl = p.parts[p.in_part]
        local choices, n = {}, 1
        for k, _ in pairs(m.commands) do choices[n], n = k, n + 1 end
        return compgen(incompl, choices)
    end,

    ---@param line string
    ---@param point number
    files = function(line, point)
        local p = line:prompt_shell_like_split(point)
        local incompl = p.parts[p.in_part]
        local choices, n = {}, 1
        local dir = incompl
        if not dir:match('/$') then dir = dir:match('(.*/)[^/]+$') or '' end
        local readdir = os.list(dir)
        if not readdir then return {} end
        for f in readdir do choices[n], n = dir .. f, n + 1 end
        return compgen(incompl, choices)
    end,

    ---@param line string
    ---@param point number
    script = function(line, point)
        local p = line:prompt_lua_tokens_split(point)
        local incompl = p.parts[p.in_part]
        local choices, n = {}, 1
        for _, val in pairs(_G)
        do
            if 'table' == type(val)
            then
                for name in pairs(val) do choices[n], n = name, n + 1 end
            end
        end
        return compgen(incompl, choices)
    end,

    ---@type table<string, fun(line:string, point:integer): string[]>
    for_command = {},
}

---@type table<string, fun(arg:string, bang:boolean)>
m.commands = {
    help = function(arg) treest:message(help(arg) or ("no help for " .. arg)) end,

    quit = function() treest:quit() end,
    cquit = function(arg) treest:quit(0 < #arg and arg or "!") end,
    suspend = function() treest:suspend() end,

    echo = function(arg, bang)
        local ok, err = load('return ' .. arg)
        if ok
        then
            _ = ok()
            if bang and nil == _ then return end
            treest:message((bang and debug.pretty or tostring)(_))
            return
        end
        treest:message(err)
    end,

    eval = function(arg)
        local ok, err = load(arg)
        if ok
        then
            _ = ok()
            return
        end
        treest:message(err)
    end,

    set = function(arg)
        local show = {}
        for _, op in ipairs(string.prompt_shell_like_split(arg).parts)
        do
            ---@type string|integer|boolean
            local val = true
            local eq = op:find('=')

            if 'no' == op:sub(1, 2)
            then
                op = op:sub(3)
                val = false
            elseif '!' == op:sub(#op)
            then
                op = op:sub(1, #op - 1)
                val = not treest:get_option(op)
            elseif '?' == op:sub(#op)
            then
                op = op:sub(1, #op - 1)
                show[#show + 1], val = op .. '=' .. tostring(treest:get_option(op))
            elseif eq
            then
                val = op:sub(eq + 1)
                op = op:sub(1, eq)
            end

            if nil ~= val then treest:set_option(op, val) end
        end

        if show[1] then treest:message(show) end
    end,

    map = function(arg)
        -- note: there cannot be spaces in lhs
        ---@type string, string?
        local lhs, rhs = arg:match('^%s*(%S+)%s(.*)%s*$')
        if not rhs or '' == rhs
        then
            lhs = arg:match('^%s*(%S+)%s*$') or arg
            local is = treest:mapped(lhs)
            treest:message(is and tostring(is) or ("no mapping for " .. lhs))
        else
            rhs:keytrans() -- assert it's a valid sequence once before mapping
            treest:map(lhs, function() treest:raw_keys(rhs) end)
        end
    end,

    unmap = function(arg)
        local lhs = arg:match('^%s*(%S+)%s*$') or arg
        if not treest:unmap(lhs) then treest:message("no mapping for " .. lhs) end
    end,

    edit = function(arg)
        --function treest:space_open(arg, name?, placement_hint?) end
        --function treest:space_close(placement?) end
    end,
}

---@param req RequestFlags
local function request(req)
    ---@param ans string
    local function request_do(ans)
        local res = treest:provider_request(req, nil, ans)
        if res then treest:message(res) end
    end
    return function(arg)
        if '' == arg
        then
            local path = treest:join_components(treest:node().components)
            local xps = req .. ' \x1b[37m' .. path .. '\x1b[m '
            treest:prompt(xps, m.completions.files, request_do)
        else
            request_do(arg)
        end
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
    for _, al in pairs { ... } do m.commands[al] = m.commands[com] end
end
alias('cquit', 'cq')
alias('echo', 'ec')
alias('eval', 'ev', 'let', 'local', 'call', 'cal')
alias('help', 'h')
alias('quit', 'q')
alias('suspend', 'sus', 'stop', 'st')
alias('set', 'se')
alias('unmap', 'unm')
alias('edit', 'ed', 'e', 'split', 'sp', 'vsplit', 'vs')

local function complete(func, ...)
    for _, com in pairs { ... } do m.completions.for_command[com] = func end
end
complete(m.completions.files,
    'edit', 'ed', 'e', 'split', 'sp', 'vsplit', 'vs',
    'mk', 'cp', 'rm', 'ch', 'vi', 'ex')
complete(m.completions.script,
    'echo', 'ec',
    'eval', 'ev', 'let', 'local', 'call', 'cal',
    'help')

local function search(q, flags)
    if not q then return end
    local found = treest:search_level(q, flags)
    if not found
    then
        treest:message("not found: " .. treest:get_register('/'))
    else
        treest:message(nil)
        treest:set_cursor(found)
        return found
    end
end

m.keys = {
    q = function() treest:quit() end,
    ZQ = function() treest:quit() end,
    ['<C-Z>'] = function() treest:suspend() end,

    [':'] = function()
        treest:prompt(':', function(line, point)
            local com = line:sub(1, point):match('(%w+)%s')
            if not com then return m.completions.commands(line, point) end
            local comp = m.completions.for_command[com]
            return comp and comp(line, point) or {}
        end, function(ans)
            local com, bang, arg = ans:match('(%w+)(!?)%s*(.*)')
            if not com then return end

            local fn = m.commands[com]
            if fn
            then
                fn(arg, '!' == bang)
            else
                treest:message("unknown command: " .. com)
            end
        end)
    end,

    ['!'] = function()
        treest:prompt('!', m.completions.files, function(ans)
            local p = assert(io.popen(ans .. ' 2>&1', 'r'))
            treest:message(assert(p:read('*a')))
            p:close()
        end)
    end,

    ['/'] = function() treest:prompt('/', function() end, function(ans) search(ans, { 'next', 'sat' }) end) end,
    ['?'] = function() treest:prompt('/', function() end, function(ans) search(ans, { 'prev', 'sat' }) end) end,
    ['n'] = function() search(treest:get_register('/'), { 'next', 'sat' }) end,
    ['N'] = function() search(treest:get_register('/'), { 'prev', 'sat' }) end,

    ['<C-E>'] = function() treest:view_down('line') end,
    ['<C-Y>'] = function() treest:view_up('line') end,
    ['<C-D>'] = function() treest:view_down('halfwin') end,
    ['<C-U>'] = function() treest:view_up('halfwin') end,
    ['<C-F>'] = function() treest:view_down('win') end,
    ['<C-B>'] = function() treest:view_up('win') end,

    ['l'] = function() treest:enter() end,
    ['h'] = function() treest:leave() end,
    ['j'] = function() treest:next('sat') end,
    ['k'] = function() treest:prev('sat') end,

    ['L'] = function() treest:unfold() end,
    ['H'] = function() treest:fold() end,
    ['<CR>'] = function()
        if treest:folded()
        then
            treest:unfold()
        else
            treest:fold()
        end
    end,

    ['<Space>'] = function()
        if treest:marked()
        then
            treest:unmark()
        else
            treest:mark()
        end
        treest:next('sat')
    end,

    ['<LeftMouse>'] = function()
        local node = treest:node_at_line(treest.mouse_event_pos.row)
        if not node then return end
        treest:set_cursor(node.path)
    end,
    ['<RightMouse>'] = function()
        local node = treest:node_at_line(treest.mouse_event_pos.row)
        if not node then return end
        if treest:folded(node.path)
        then
            treest:unfold(node.path)
        else
            treest:fold(node.path)
        end
    end,

    ['<BackwardWheel>'] = function()
        if false -- TODO: term_row - treest.mouse_event_pos.row < treest:get_option('msh')
        then
            treest:message_scroll_down('mouse')
        else
            treest:view_down('mouse')
        end
    end,
    ['<ForwardWheel>'] = function()
        if false -- TODO: term_row - treest.mouse_event_pos.row < treest:get_option('msh')
        then
            treest:message_scroll_up('mouse')
        else
            treest:view_up('mouse')
        end
    end,

    ['['] = function() treest:message_scroll_up('line') end,
    [']'] = function() treest:message_scroll_down('line') end,
    ['{'] = function() treest:message_scroll_up('halfwin') end,
    ['}'] = function() treest:message_scroll_down('halfwin') end,

    ['<C-L>'] = function()
        treest:message({})
        treest:force_redraw()
    end,
}

m.init = function()
    treest:set_option('mouse', true)
    treest:set_option('altscreen', true)
    for seq, cb in pairs(m.keys) do treest:map(seq, cb) end
    treest:unfold()
end

return m
