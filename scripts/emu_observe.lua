-- Observe a ROM in FCEUX: run N frames (optional button press), then print palette,
-- a compact nametable grid, OAM sprites and a few RAM bytes. Output is text for the reader agent.
local out = os.getenv("NESOBS_OUT"); local frames = tonumber(os.getenv("NESOBS_FRAMES") or "240")
local press = os.getenv("NESOBS_PRESS"); local press_at = tonumber(os.getenv("NESOBS_PRESS_AT") or "0")
if press == "" then press = nil end
-- Count PPUDATA writes by VRAM region (works even when the tiles written are zero, e.g. stub assets).
local vaddr, latch, vinc = 0, 0, 1
local wr = { nt = 0, at = 0, pal = 0, pat = 0 }
local rowwr = {}; for r = 0, 29 do rowwr[r] = 0 end
local last_ctrl, last_mask = 0, 0
memory.registerwrite(0x2000, 1, function(a, s, v) last_ctrl = v; vinc = (math.floor(v / 4) % 2 == 1) and 32 or 1 end)
memory.registerwrite(0x2001, 1, function(a, s, v) last_mask = v end)
memory.registerwrite(0x2005, 1, function(a, s, v) latch = 1 - latch end)
memory.registerwrite(0x2006, 1, function(a, s, v)
  if latch == 0 then vaddr = (v % 64) * 256 + (vaddr % 256); latch = 1 else vaddr = vaddr - (vaddr % 256) + v; latch = 0 end
end)
memory.registerwrite(0x2007, 1, function(a, s, v)
  local x = vaddr % 0x4000
  if x >= 0x3F00 then wr.pal = wr.pal + 1 elseif x >= 0x2000 then if (x % 0x400) >= 0x3C0 then wr.at = wr.at + 1 else wr.nt = wr.nt + 1; local r = math.floor((x % 0x400) / 32); if x < 0x2400 then rowwr[r] = rowwr[r] + 1 end end else wr.pat = wr.pat + 1 end
  vaddr = vaddr + vinc
end)
for i = 1, frames do
  if press and ((i >= press_at and i <= press_at + 2) or (i >= press_at + 40 and i <= press_at + 42)) then
    local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j)
  end
  emu.frameadvance()
end
local f = io.open(out .. ".part", "w")
local function rd(a) if ppu and ppu.readbyte then return ppu.readbyte(a) end return memory.readbyte(a) end
f:write(string.format("frame %d\n", frames))
local pal = {}; for i = 0, 31 do pal[#pal+1] = string.format("%02X", rd(0x3F00 + i)) end
f:write("PALETTE (bg0 bg1 bg2 bg3 | spr0 spr1 spr2 spr3): " .. table.concat(pal, " ") .. "\n")
f:write(string.format("PPU CTRL=%02X (bg pattern table %d, sprite pattern table %d, sprites %s, NMI %s, nametable %d)  MASK=%02X (bg %s, sprites %s)\n", last_ctrl, math.floor(last_ctrl/16)%2, math.floor(last_ctrl/8)%2, (math.floor(last_ctrl/32)%2==1) and "8x16" or "8x8", (last_ctrl>=128) and "on" or "off", last_ctrl%4, last_mask, (math.floor(last_mask/8)%2==1) and "on" or "off", (math.floor(last_mask/16)%2==1) and "on" or "off"))
f:write("NAMETABLE 0 (32x30 tile ids, hex; '..' = 00, '__' = 20):\n")
for row = 0, 29 do local t = {} for col = 0, 31 do local v = rd(0x2000 + row*32 + col); t[#t+1] = (v == 0) and ".." or ((v == 0x20) and "__" or string.format("%02X", v)) end f:write(string.format("%02d %s\n", row, table.concat(t, " "))) end
f:write(string.format("VRAM WRITES (PPUDATA bytes by region over the run): nametable=%d attribute=%d palette=%d pattern=%d\n", wr.nt, wr.at, wr.pal, wr.pat))
f:write("NT0 ROWS WRITTEN (row:bytes, rows the code wrote at least once): "); for r = 0, 29 do if rowwr[r] > 0 then f:write(string.format("%02d:%d ", r, rowwr[r])) end end f:write("\n")
f:write("ATTRIBUTES 0 (one line per attribute row = 4 tile rows; byte = 2x2 groups of 2x2 tiles, bits 0-1 top-left, 2-3 top-right, 4-5 bottom-left, 6-7 bottom-right):\n")
for ar = 0, 7 do local t = {} for i = 0, 7 do t[#t+1] = string.format("%02X", rd(0x23C0 + ar*8 + i)) end
  f:write(string.format("  attr row %d (tile rows %02d-%02d): %s\n", ar, ar*4, ar*4+3, table.concat(t, " "))) end
f:write("SPRITES (index: x y tile attr) visible only:\n"); local n = 0
for i = 0, 63 do local y = memory.readbyte(0x0200 + i*4); local t = memory.readbyte(0x0201 + i*4); local a = memory.readbyte(0x0202 + i*4); local x = memory.readbyte(0x0203 + i*4)
  if y < 0xEF then n = n + 1; if n <= 32 then f:write(string.format("  %02d: x=%3d y=%3d tile=%02X attr=%02X\n", i, x, y, t, a)) end end end
f:write(string.format("  (%d visible sprites; OAM read from $0200 shadow — may be elsewhere)\n", n))
f:write("RAM $0000-$00FF: "); for i = 0, 255 do f:write(string.format("%02X", memory.readbyte(i))); if i % 32 == 31 then f:write("\n                 ") end end f:write("\n")
f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

