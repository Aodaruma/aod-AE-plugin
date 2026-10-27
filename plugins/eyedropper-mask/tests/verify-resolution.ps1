param([string]$OutputDirectory)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$taskOutput=if($OutputDirectory){$OutputDirectory}else{Join-Path $taskRoot 'target/ae-gap-test'}
$taskCases=Import-Csv (Join-Path $taskOutput 'host-cases.csv')
if($taskCases.Count -ne 71){throw "Expected 71 renders; found $($taskCases.Count)"}
$taskResults=@()
foreach($taskCase in $taskCases){
    $taskImage=[Drawing.Bitmap]::FromFile((Join-Path $taskOutput "$($taskCase.Case).png"))
    try {
        $taskLeft=$taskImage.GetPixel([int][Math]::Floor($taskImage.Width/8),[int][Math]::Floor($taskImage.Height/2))
        $taskRight=$taskImage.GetPixel([int][Math]::Floor($taskImage.Width*7/8),[int][Math]::Floor($taskImage.Height/2))
        $taskSizeOK=$taskImage.Width -eq [Math]::Ceiling(256.0/[int]$taskCase.Factor) -and $taskImage.Height -eq [Math]::Ceiling(128.0/[int]$taskCase.Factor)
        $taskExpected=[int]$taskCase.ExpectedRightAlpha
        if($taskCase.Case.StartsWith('narrow-') -and [int]$taskCase.Stable -eq 1){$taskExpected=255}
        $taskPass=$taskSizeOK -and $taskLeft.A -le 1 -and ($taskExpected -lt 0 -or [Math]::Abs($taskRight.A-$taskExpected) -le 1)
        $taskResults+=[pscustomobject]@{Case=$taskCase.Case;LeftAlpha=$taskLeft.A;RightAlpha=$taskRight.A;Expected=$taskExpected;Pass=$taskPass}
    } finally {$taskImage.Dispose()}
}
# Full must be byte-identical with protection ON or OFF.
foreach($taskDepth in @(8,16,32)){
    foreach($taskSource in @(1,2)){
        $taskOff=(Get-FileHash (Join-Path $taskOutput "narrow-bpc$taskDepth-source$taskSource-stable0-res1.png")).Hash
        $taskOn=(Get-FileHash (Join-Path $taskOutput "narrow-bpc$taskDepth-source$taskSource-stable1-res1.png")).Hash
        if($taskOff -ne $taskOn){throw "Full output changed: $taskDepth bpc / source $taskSource"}
    }
}
$taskResults | Export-Csv (Join-Path $taskOutput 'pixel-results.csv') -NoTypeInformation -Encoding utf8
$taskResults | Format-Table -AutoSize
if(@($taskResults | Where-Object {-not $_.Pass}).Count){throw 'Host image comparison failed'}
'PASS: 71 renders; local gap suppression, wide openings, unrecoverable boundaries, alpha, Source/Input, radius control, and unchanged Full verified.' | Tee-Object -FilePath (Join-Path $taskOutput 'verification.txt')
