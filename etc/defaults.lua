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

    cquit = function(arg) treest:quit(0 < #arg and arg or "!") end,
    qall = function() treest:quit() end,
    suspend = function() treest:suspend() end,

    quit = function()
        --if not has spaces then treest:quit() end
        treest:space_close()
        -- TODO(wip)
        --if last
        --then
        --    treest:quit()
        --else
        treest:force_redraw()
        --end
    end,

    unload = function()
        -- TODO(wip)
        --if has spaces
        --then
        treest:space_close()
        treest:force_redraw()
        --end
    end,

    edit = function(arg)
        ---@type string?
        local name
        local st, ed = arg:find('%+%l+%s*$')
        if st and ed then arg, name = arg:sub(1, st - 1), arg:sub(st, ed) end
        local aarg = arg:match('^%s*(.-)%s*$')
        if not aarg then return end -- TODO: force reload
        treest:space_open(aarg, name)
    end,

    echo = function(arg, bang)
        local ok, err = load('return ' .. arg)
        if not ok then return treest:message(err) end
        _ = ok()
        if bang and nil == _ then return end
        treest:message((bang and debug.pretty or tostring)(_))
    end,

    eval = function(arg)
        local ok, err = load(arg)
        if not ok then return treest:message(err) end
        _ = ok()
    end,

    func = function(arg)
        if arg:find('%(')
        then
            local ok, err = load('function ' .. arg)
            if not ok then return treest:message(err) end
            _ = ok()
        else
            local ok, err = load('return ' .. arg)
            if not ok then return treest:message(err) end
            _ = ok()
            treest:message(tostring(_))
        end
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
                val = not treest:option_get(op)
            elseif '?' == op:sub(#op)
            then
                op = op:sub(1, #op - 1)
                show[#show + 1], val = op .. '=' .. tostring(treest:option_get(op))
            elseif eq
            then
                val = op:sub(eq + 1)
                op = op:sub(1, eq)
            end

            if nil ~= val then treest:option_set(op, val) end
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
            local is = treest:key_mapped(lhs)
            treest:message(is and tostring(is) or ("no mapping for " .. lhs))
        else
            rhs:key_trans() -- assert it's a valid sequence once before mapping
            treest:key_map(lhs, function() treest:key_raw(rhs) end)
        end
    end,

    unmap = function(arg)
        local lhs = arg:match('^%s*(%S+)%s*$') or arg
        if not treest:key_unmap(lhs) then treest:message("no mapping for " .. lhs) end
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
            local path = treest:provider_join_components(treest:node_info().components)
            local xps = req .. ' \x1b[37m' .. path .. '\x1b[m '
            treest:register_prompt(xps, m.completions.files, request_do)
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
alias('edit', 'ed', 'e', 'split', 'sp', 'vsplit', 'vs')
alias('eval', 'ev', 'let', 'local', 'call', 'cal')
alias('function', 'func', 'fu')
alias('help', 'h')
alias('qall', 'qa')
alias('quit', 'q')
alias('set', 'se')
alias('suspend', 'sus', 'stop', 'st')
alias('unload', 'bd', 'bdel', 'bdelete', 'bun', 'bunload')
alias('unmap', 'unm')

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
        treest:message("not found: " .. treest:register_get('/'))
    else
        treest:message(nil)
        treest:cursor_set(found)
        return found
    end
end

m.keys = {
    q = function() treest:quit() end,
    ZQ = function() treest:quit() end,
    ['<C-Z>'] = function() treest:suspend() end,

    [':'] = function()
        treest:register_prompt(':', function(line, point)
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
        treest:register_prompt('!', m.completions.files, function(ans)
            local p = assert(io.popen(ans .. ' 2>&1', 'r'))
            treest:message(assert(p:read('*a')))
            p:close()
        end)
    end,

    ['/'] = function() treest:register_prompt('/', function() end, function(ans) search(ans, { 'next', 'sat' }) end) end,
    ['?'] = function() treest:register_prompt('/', function() end, function(ans) search(ans, { 'prev', 'sat' }) end) end,
    ['n'] = function() search(treest:register_get('/'), { 'next', 'sat' }) end,
    ['N'] = function() search(treest:register_get('/'), { 'prev', 'sat' }) end,

    ['<C-E>'] = function() treest:view_down('line') end,
    ['<C-Y>'] = function() treest:view_up('line') end,
    ['<C-D>'] = function() treest:view_down('halfwin') end,
    ['<C-U>'] = function() treest:view_up('halfwin') end,
    ['<C-F>'] = function() treest:view_down('win') end,
    ['<C-B>'] = function() treest:view_up('win') end,

    ['l'] = function() treest:node_enter() end,
    ['h'] = function() treest:node_leave() end,
    ['j'] = function() treest:node_next('sat') end,
    ['k'] = function() treest:node_prev('sat') end,

    ['L'] = function() treest:node_unfold() end,
    ['H'] = function() treest:node_fold() end,
    ['<CR>'] = function()
        if treest:node_folded()
        then
            treest:node_unfold()
        else
            treest:node_fold()
        end
    end,

    ['<Space>'] = function()
        if treest:node_marked()
        then
            treest:node_unmark()
        else
            treest:node_mark()
        end
        treest:node_next('sat')
    end,

    ['<LeftMouse>'] = function()
        local node = treest:node_info_at_line(treest.mouse_event_pos.row)
        if not node then return end
        treest:cursor_set(node.path)
    end,
    ['<RightMouse>'] = function()
        local node = treest:node_info_at_line(treest.mouse_event_pos.row)
        if not node then return end
        if treest:node_folded(node.path)
        then
            treest:node_unfold(node.path)
        else
            treest:node_fold(node.path)
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
    treest:option_set('mouse', true)
    treest:option_set('altscreen', true)
    for seq, cb in pairs(m.keys) do treest:key_map(seq, cb) end
    treest:node_unfold()
end

return m
