package.preload['defaults'] = function()
    local m = {}

    m.keys = {
        q= function() treest:quit() end

        -- TODO ofc
    }

    m.init = function()
        for seq, cb in pairs(m.keys)
          do treest:map(seq, cb)
        end
        treest:unfold(--[[treest.cursor]])
    end

    return m
end
