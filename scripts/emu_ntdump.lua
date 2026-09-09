-- Dump nametable 0 (32x30) as a text grid of hex tile ids after N frames.
local out = os.getenv("NESDUMP_OUT") or "nt.txt"; local frames = tonumber(os.getenv("NESDUMP_FRAMES") or "240")
for i = 1, frames do emu.frameadvance() end
local f = io.open(out .. ".part", "w")
local function rd(a) if ppu and ppu.readbyte then return ppu.readbyte(a) end return memory.readbyte(a) end
for row = 0, 29 do local t = {} for col = 0, 31 do t[#t+1] = string.format("%02X", rd(0x2000 + row*32 + col)) end f:write(string.format("%02d: %s\n", row, table.concat(t, " "))) end
f:write("ATTR: ") for i = 0, 63 do f:write(string.format("%02X ", rd(0x23C0 + i))) end f:write("\n")
f:close(); os.rename(out .. ".part", out); print("dumped")
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

