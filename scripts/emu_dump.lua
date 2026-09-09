-- FCEUX Lua: run N frames, then dump palette + nametable-0 non-blank tiles to a text file.
local out = os.getenv("NESDUMP_OUT") or "dump.txt"
local frames = tonumber(os.getenv("NESDUMP_FRAMES") or "240")
for i = 1, frames do emu.frameadvance() end
local f = io.open(out .. ".part", "w")
local function rd(a) if ppu and ppu.readbyte then return ppu.readbyte(a) end return memory.readbyte(a) end
f:write("PPUCTRL/MASK shadow unknown; palette:\n")
local pal = {}
for i = 0, 31 do pal[#pal+1] = string.format("%02X", rd(0x3F00 + i)) end
f:write(table.concat(pal, " ") .. "\n")
local counts = {}
local nonblank = 0
for a = 0x2000, 0x23BF do
  local v = rd(a); counts[v] = (counts[v] or 0) + 1
  if v ~= 0x20 and v ~= 0x00 then nonblank = nonblank + 1; if nonblank <= 40 then f:write(string.format("nt %04X = %02X '%s'\n", a, v, (v >= 32 and v < 127) and string.char(v) or "?")) end end
end
f:write("nonblank tiles: " .. nonblank .. "\n")
for v, c in pairs(counts) do if c > 100 then f:write(string.format("tile %02X x%d\n", v, c)) end end
f:write(string.format("cpu zp: nmi_enabled? RAM[0x00..0x0F]="))
for i = 0, 15 do f:write(string.format("%02X ", memory.readbyte(i))) end
f:write("\n"); f:close(); os.rename(out .. ".part", out); print("dumped " .. out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

