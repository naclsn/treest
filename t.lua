local mt = {__index= table}
---@param t table
---@return table
function T(t) return setmetatable(t, mt) end

---Collect from a single-value iterator function.
---
---@generic I
---@param iterator fun(...): I, ...
---@return I[]
function tolist(iterator, ...)
    local r, n = T{}, 1
    for v in iterator, ... do r[n], n = v, n+1 end
    return r
end

---Collect from a pairs-like iterator function.
---
---@generic K, I
---@param iterator fun(...): K, I, ...
---@return table<K, I>
function totable(iterator, ...)
    local r = T{}
    for k, v in iterator, ... do r[k] = v end
    return r
end

---Duplicate the table shallowly.
---
---@param table table
---@return table
function table.copy(table)
    return totable(ipairs(table))
end

---Duplicate the table recursively.
---
---@param object table
---@return table
function table.deepcopy(object)
    local r = T{}
    for key, value in pairs(object) do r[key] = 'table' == type(value) and table.deepcopy(value) or value end
    return r
end

---Make a new list with all the keys.
---
---@generic K
---@param table table<K, any>
---@return K[]
function table.keys(table)
    return tolist(pairs(table))
end

---Make a new list with only the integer keys.
---
---@generic K
---@param table table<K, any>
---@return K[]
function table.ikeys(table)
    return tolist(ipairs(table))
end

---Make a new list with only the values.
---
---@generic I
---@param table table<any, I>
---@return I[]
function table.values(table)
    local r, n = T{}, 1
    for _, v in pairs(table) do r[n], n = v, n+1 end
    return r
end

---Update the table inplace with the key/value pairs from other, overwriting existing ones.
---
---@generic T
---@param table T
---@param other table
---@return T
function table.update(table, other)
    local r = T{}
    for k, v in ipairs(other) do table[k] = v end
    return r
end

---Executes the given f over all elements of table. For each element, f is called with only the value as arguments. The results are stored and return as a matching table.
---
---@generic I, O
---@param list I[]
---@param callback fun(value: I): O
---@return O[]
function table.map(list, callback)
    local r = T{}
    for k, v in ipairs(list) do r[k] = callback(v) end
    return r
end

---Executes the given f over all elements of table. For each element, f is called with the index and respective value as arguments. The results are stored and return as a matching table.
---
---@generic I, O
---@param list I[]
---@param callback fun(key: integer, value: I): O
---@return O[]
function table.map2(list, callback)
    local r = T{}
    for k, v in ipairs(list) do r[k] = callback(k, v) end
    return r
end

---Executes the given f over all elements of table. For each element, f is called only the value as arguments. Only the elements passing the predicate are retained in the new returned table.
---
---@generic I
---@param list I[]
---@param predicate fun(value: I): boolean
---@return I[]
function table.filter(list, predicate)
    local r, n = T{}, 1
    for _, v in ipairs(list) do if predicate(v) then r[n], n = v, n+1 end end
    return r
end

---Executes the given f over all elements of table. For each element, f is called with the index and respective value as arguments. Only the elements passing the predicate are retained in the new returned table.
---
---@generic I
---@param list I[]
---@param predicate fun(key: integer, value: I): boolean
---@return I[]
function table.filter2(list, predicate)
    local r, n = T{}, 1
    for k, v in ipairs(list) do if predicate(k, v) then r[n], n = v, n+1 end end
    return r
end

---Reverse into a new list.
---
---@generic I
---@param list I[]
---@return I[]
function table.reverse(list)
    local r, m = T{}, #list
    for k, v in ipairs(list) do r[m-k+1] = v end
    return r
end

---Make a new list of numbers from start to stop, simply using the `for =n,m[,l]` syntax.
---
---@param start number
---@param stop number
---@param step number?
---@return number[]
function table.range(start, stop, step)
    local r, n = T{}, 1
    if step
        then for k=start,stop,step do r[n], n = k, n+1 end
        else for k=start,stop do r[n], n = k, n+1 end
    end
    return r
end
