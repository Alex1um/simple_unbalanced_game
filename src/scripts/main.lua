for k, v in pairs(ships) do
    print(k, ":")
    for kv, vv in pairs(v) do
        print("  ", kv, vv)
    end
end