# memory-pressure.ps1
# Gradually allocates memory until ~90% of system RAM is in use.
# Press Enter to release and exit.

$os = Get-CimInstance Win32_OperatingSystem
$totalMB = [math]::Round($os.TotalVisibleMemorySize / 1024)
$toAllocMB = [math]::Round($totalMB * 0.9)

Write-Host "Total RAM:       $totalMB MB"
Write-Host "Will allocate:   ~$toAllocMB MB (90% of total) in 512 MB chunks"
Write-Host "This will cause heavy memory pressure on ALL processes."
Write-Host ""

$chunks = [System.Collections.Generic.List[byte[]]]::new()
$allocatedMB = 0
$chunkSize = 512MB

while ($allocatedMB -lt $toAllocMB) {
    $remaining = [long]($toAllocMB - $allocatedMB) * 1MB
    $size = [math]::Min([long]$chunkSize, [long]$remaining)
    try {
        $chunk = [byte[]]::new($size)
        # Touch every page (4KB) to force commit
        for ($i = 0; $i -lt $chunk.Length; $i += 4096) {
            $chunk[$i] = 1
        }
        $chunks.Add($chunk)
        $allocatedMB += [math]::Round($size / 1MB)
        Write-Host "  Allocated $allocatedMB / $toAllocMB MB"
    } catch {
        Write-Host "  Allocation failed at $allocatedMB MB (out of memory). Stopping."
        break
    }
}

Write-Host ""
Write-Host "Memory pressure active. Check your plugin's RSS in Task Manager now."
Write-Host "Press Enter to release all memory and exit..."
Read-Host | Out-Null

$chunks.Clear()
[GC]::Collect()
Write-Host "Released. Done."
