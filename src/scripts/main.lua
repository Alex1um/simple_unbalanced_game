for id, ship in pairs(ships) do
    -- for kv, vv in pairs(v) do
    --     print("  ", kv, vv)
    -- end
    if ship.current_angle ~= ship.angle then
        ship.x = ship.x + math.cos(ship.current_angle) * ship.v
        ship.y = ship.y + math.sin(ship.current_angle) * ship.v
    end
end