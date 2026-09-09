-- Per-frame CPU profile by routine: attributes cycles to the most recently entered C/asm symbol.
-- Env: NESPROF_OUT, NESPROF_MAP (ld65 map), NESPROF_FROM, NESPROF_TO, NESPROF_PRESS, NESPROF_AT, NESPROF_TOP
local out = os.getenv("NESPROF_OUT"); local mapf = os.getenv("NESPROF_MAP")
local f0 = tonumber(os.getenv("NESPROF_FROM") or "60"); local f1 = tonumber(os.getenv("NESPROF_TO") or "120")
local press = os.getenv("NESPROF_PRESS"); local at = tonumber(os.getenv("NESPROF_AT") or "0"); if press == "" then press = nil end
local top = tonumber(os.getenv("NESPROF_TOP") or "16")
local syms = {}
for line in io.lines(mapf) do
  for name, addr in line:gmatch("(_[%w_]+)%s+(%x%x%x%x%x%x)") do
    local a = tonumber(addr, 16)
    if a >= 0x8000 and a <= 0xFFFF then syms[#syms + 1] = { name = name, addr = a } end
  end
end
local cur, last_t = "(boot)", 0
local total, frames = {}, 0
local function attribute(now) total[cur] = (total[cur] or 0) + (now - last_t); last_t = now end
for _, s in ipairs(syms) do
  memory.registerexecute(s.addr, 1, function() local now = debugger.getcyclescount(); if frames >= 0 then attribute(now) end; cur = s.name end)
end
local frame = 0; local active = false
while frame <= f1 do
  if press and frame >= at and frame <= at + 2 then local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j) end
  if frame == f0 then active = true; total = {}; last_t = debugger.getcyclescount(); frames = 0 end
  emu.frameadvance()
  if active then attribute(debugger.getcyclescount()); frames = frames + 1 end
  frame = frame + 1
end
local rows = {}; local sum = 0
for k, v in pairs(total) do rows[#rows + 1] = { k, v }; sum = sum + v end
table.sort(rows, function(a, b) return a[2] > b[2] end)
local f = io.open(out .. ".part", "w")
f:write(string.format("frames %d-%d (%d frames), %d CPU cycles total = %d per frame (budget 29780/frame incl. NMI; anything above means a lag frame)\n", f0, f1, frames, sum, math.floor(sum / math.max(frames, 1))))
f:write("cycles/frame attributed to the most recently entered routine (callees NOT included unless they are symbols too):\n")
for i = 1, math.min(top, #rows) do f:write(string.format("  %-32s %7d cycles/frame  (%4.1f%%)\n", rows[i][1], math.floor(rows[i][2] / math.max(frames, 1)), 100 * rows[i][2] / math.max(sum, 1))) end
f:close(); os.rename(out .. ".part", out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

