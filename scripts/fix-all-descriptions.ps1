# PowerShell script to fix all description files by adding parameter names from schema files

$toolsDir = "d:\项目\agent\wf-agent\sdk\resources\predefined\tools"

# Find all description.ts files
$descFiles = Get-ChildItem -Path $toolsDir -Filter "description.ts" -Recurse

Write-Host "Found $($descFiles.Count) description files"

$fixedCount = 0

foreach ($descFile in $descFiles) {
    $schemaFile = Join-Path (Split-Path $descFile.FullName) "schema.ts"
    
    if (-not (Test-Path $schemaFile)) {
        Write-Host "No schema for: $($descFile.Name)" -ForegroundColor Yellow
        continue
    }
    
    # Read description file
    $descContent = Get-Content $descFile.FullName -Raw
    
    # Check if already has name fields in parameters
    if ($descContent -match 'parameters:\s*\[[\s\S]*?name:\s*"') {
        Write-Host "Already fixed: $($descFile.Name)" -ForegroundColor Gray
        continue
    }
    
    # Read schema file and extract parameter names
    $schemaContent = Get-Content $schemaFile -Raw
    
    # Extract property names from schema
    if ($schemaContent -match 'properties:\s*\{([^}]+)\}') {
        $propBlock = $matches[1]
        $paramNames = [regex]::Matches($propBlock, '(\w+):\s*\{') | ForEach-Object { $_.Groups[1].Value }
        
        if ($paramNames.Count -eq 0) {
            Write-Host "No params in schema: $($descFile.Name)" -ForegroundColor Yellow
            continue
        }
        
        # Add name fields to parameters
        $lines = $descContent -split "`n"
        $result = @()
        $inParameters = $false
        $paramIndex = 0
        
        for ($i = 0; $i -lt $lines.Length; $i++) {
            $line = $lines[$i]
            
            if ($line -match 'parameters:\s*\[') {
                $inParameters = $true
                $result += $line
                continue
            }
            
            if ($inParameters) {
                # Check if this is a parameter object start (line with just "{")
                if ($line.Trim() -eq '{' -and $i + 1 -lt $lines.Length -and $lines[$i + 1] -match 'type:') {
                    if ($paramIndex -lt $paramNames.Count) {
                        $result += $line
                        $result += "      name: `"$($paramNames[$paramIndex])`","
                        $paramIndex++
                        continue
                    }
                }
                
                # Check if we've exited the parameters array
                if ($line -match '\],') {
                    $inParameters = $false
                }
            }
            
            $result += $line
        }
        
        $fixedContent = $result -join "`n"
        
        if ($fixedContent -ne $descContent) {
            Set-Content -Path $descFile.FullName -Value $fixedContent -NoNewline
            Write-Host "Fixed: $($descFile.Name) ($paramIndex params)" -ForegroundColor Green
            $fixedCount++
        }
    }
}

Write-Host "`nFixed $fixedCount files" -ForegroundColor Cyan
