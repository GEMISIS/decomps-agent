-- Log selected RAM bytes per frame. Env: NESRAM_OUT, NESRAM_ADDRS (comma list, hex like 22,1A,0300), NESRAM_FROM, NESRAM_TO, NESRAM_PRESS, NESRAM_AT
local out = os.getenv("NESRAM_OUT"); local from = tonumber(os.getenv("NESRAM_FROM") or "1"); local to = tonumber(os.getenv("NESRAM_TO") or "120")
local press = os.getenv("NESRAM_PRESS"); local at = tonumber(os.getenv("NESRAM_AT") or "0"); local hold = tonumber(os.getenv("NESRAM_HOLD") or "3"); if press == "" then press = nil end
local addrs = {}; for a in (os.getenv("NESRAM_ADDRS") or "00"):gmatch("[0-9A-Fa-f]+") do addrs[#addrs + 1] = tonumber(a, 16) end
local lines = {}; local prev = nil
for i = 1, to do
  if press and i >= at and i < at + hold then local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j) end
  emu.frameadvance()
  if i >= from then
    local s = ""; for _, a in ipairs(addrs) do s = s .. string.format(" $%04X=%02X", a, memory.readbyte(a)) end
    if s ~= prev then lines[#lines + 1] = string.format("f%d%s", i, s); prev = s
    elseif i == to then lines[#lines + 1] = string.format("f%d (unchanged since last line)", i) end
  end
end
local f = io.open(out .. ".part", "w"); f:write("(only frames where a watched byte changed are listed)\n" .. table.concat(lines, "\n") .. "\n"); f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

