#requires -Version 5.1
<#
.SYNOPSIS
Runs the Dockwind local quality gate.

.DESCRIPTION
Runs formatting, check, clippy, tests, the release build, and local Markdown
file-link checks. The script resolves the repository root from its own path, so
it works from any current directory. It stops at the first failed native command.

Markdown checks use actual links, not paths written in code spans or fenced
blocks. The parser tracks CommonMark fence characters and run lengths, masks
inline code spans including multiline spans, and parses balanced link labels and
destinations. External, plugin, and skill URIs are skipped. Anchor fragments are
ignored when the file part is checked. The script never creates files.
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
Set-Location -LiteralPath $repoRoot

$referenceDefinitionPattern = [regex]'(?m)^[ \t]{0,3}\[(?<label>[^\]]+)\]:\s*(?<target><[^>]*>|\S+)'

function Stop-Check {
    param([string]$Message)

    Write-Host "[check] FAILED: $Message" -ForegroundColor Red
    exit 1
}

function Invoke-NativeCommand {
    param(
        [string]$Command,
        [string[]]$Arguments
    )

    $display = "$Command $($Arguments -join ' ')"
    Write-Host "[check] $display"

    try {
        & $Command @Arguments
    }
    catch {
        Stop-Check "$display could not start: $($_.Exception.Message)"
    }

    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        Write-Host "[check] FAILED: $display (exit $exitCode)" -ForegroundColor Red
        exit $exitCode
    }
}

function Set-Mask {
    param(
        [char[]]$Chars,
        [int]$Start,
        [int]$End
    )

    $limit = [Math]::Min($End, $Chars.Length)
    for ($index = $Start; $index -lt $limit; $index++) {
        if ($Chars[$index] -ne [char]10 -and $Chars[$index] -ne [char]13) {
            $Chars[$index] = [char]32
        }
    }
}

function Test-IsEscaped {
    param(
        [string]$Text,
        [int]$Index
    )

    $slashes = 0
    $cursor = $Index - 1
    while ($cursor -ge 0 -and $Text[$cursor] -eq [char]92) {
        $slashes++
        $cursor--
    }

    return (($slashes % 2) -eq 1)
}

function Hide-FencedCode {
    param(
        [char[]]$Chars,
        [string]$Text
    )

    $inFence = $false
    $fenceChar = [char]0
    $fenceLength = 0
    $index = 0

    while ($index -lt $Text.Length) {
        $lineStart = $index
        $lineEnd = $index
        while ($lineEnd -lt $Text.Length -and $Text[$lineEnd] -ne [char]10 -and $Text[$lineEnd] -ne [char]13) {
            $lineEnd++
        }

        $line = $Text.Substring($lineStart, $lineEnd - $lineStart)

        if ($inFence) {
            $closingPattern = '^[ ]{0,3}' + [regex]::Escape([string]$fenceChar) + '{' + $fenceLength + ',}[ \t]*$'
            if ([regex]::IsMatch($line, $closingPattern)) {
                $inFence = $false
            }
            Set-Mask -Chars $Chars -Start $lineStart -End $lineEnd
        }
        else {
            $opening = [regex]::Match($line, '^[ ]{0,3}(`{3,}|~{3,})(.*)$')
            if ($opening.Success) {
                $run = $opening.Groups[1].Value
                $info = $opening.Groups[2].Value
                $isBacktickFence = $run[0] -eq [char]96
                if (-not ($isBacktickFence -and $info.Contains('`'))) {
                    $fenceChar = $run[0]
                    $fenceLength = $run.Length
                    $inFence = $true
                    Set-Mask -Chars $Chars -Start $lineStart -End $lineEnd
                }
            }
        }

        $index = $lineEnd
        if ($index -lt $Text.Length) {
            if ($Text[$index] -eq [char]13 -and $index + 1 -lt $Text.Length -and $Text[$index + 1] -eq [char]10) {
                $index += 2
            }
            else {
                $index++
            }
        }
    }
}

function Hide-InlineCode {
    param(
        [char[]]$Chars,
        [string]$Text
    )

    $index = 0
    while ($index -lt $Text.Length) {
        if ($Chars[$index] -ne [char]96) {
            $index++
            continue
        }

        if (Test-IsEscaped -Text $Text -Index $index) {
            $index++
            continue
        }

        $runStart = $index
        $runLength = 0
        while ($index -lt $Text.Length -and $Chars[$index] -eq [char]96) {
            $runLength++
            $index++
        }

        $search = $index
        $closingStart = -1
        $closingEnd = -1
        while ($search -lt $Text.Length) {
            if ($Chars[$search] -ne [char]96) {
                $search++
                continue
            }

            $candidateStart = $search
            $candidateLength = 0
            while ($search -lt $Text.Length -and $Chars[$search] -eq [char]96) {
                $candidateLength++
                $search++
            }

            if ($candidateLength -eq $runLength) {
                $closingStart = $candidateStart
                $closingEnd = $search
                break
            }
        }

        if ($closingStart -ge 0) {
            Set-Mask -Chars $Chars -Start $runStart -End $closingEnd
            $index = $closingEnd
        }
    }
}

function Hide-HtmlComments {
    param(
        [char[]]$Chars,
        [string]$Text
    )

    foreach ($match in [regex]::Matches($Text, '(?s)<!--.*?-->')) {
        Set-Mask -Chars $Chars -Start $match.Index -End ($match.Index + $match.Length)
    }
}

function Read-LinkTitle {
    param(
        [string]$Text,
        [int]$Start
    )

    $opening = $Text[$Start]
    if ($opening -eq [char]34 -or $opening -eq [char]39) {
        $index = $Start + 1
        while ($index -lt $Text.Length) {
            if ($Text[$index] -eq [char]92) {
                $index += 2
                continue
            }
            if ($Text[$index] -eq $opening) {
                return @{ Success = $true; End = $index + 1 }
            }
            $index++
        }
        return @{ Success = $false }
    }

    if ($opening -eq [char]40) {
        $index = $Start + 1
        $depth = 1
        while ($index -lt $Text.Length) {
            if ($Text[$index] -eq [char]92) {
                $index += 2
                continue
            }
            if ($Text[$index] -eq [char]40) {
                $depth++
            }
            elseif ($Text[$index] -eq [char]41) {
                $depth--
                if ($depth -eq 0) {
                    return @{ Success = $true; End = $index + 1 }
                }
            }
            $index++
        }
    }

    return @{ Success = $false }
}

function Read-LinkDestination {
    param(
        [string]$Text,
        [int]$Start
    )

    $index = $Start
    while ($index -lt $Text.Length -and [char]::IsWhiteSpace($Text[$index])) {
        $index++
    }
    if ($index -ge $Text.Length) {
        return @{ Success = $false }
    }

    if ($Text[$index] -eq [char]60) {
        $index++
        $destinationStart = $index
        $closed = $false
        while ($index -lt $Text.Length) {
            if ($Text[$index] -eq [char]92) {
                $index += 2
                continue
            }
            if ($Text[$index] -eq [char]62) {
                $closed = $true
                break
            }
            if ($Text[$index] -eq [char]10 -or $Text[$index] -eq [char]13) {
                return @{ Success = $false }
            }
            $index++
        }
        if (-not $closed) {
            return @{ Success = $false }
        }
        $destination = $Text.Substring($destinationStart, $index - $destinationStart)
        $index++
    }
    else {
        $destinationStart = $index
        $depth = 0
        while ($index -lt $Text.Length) {
            if ($Text[$index] -eq [char]92) {
                $index += 2
                continue
            }
            if ($Text[$index] -eq [char]40) {
                $depth++
                $index++
                continue
            }
            if ($Text[$index] -eq [char]41) {
                if ($depth -eq 0) {
                    break
                }
                $depth--
                $index++
                continue
            }
            if ([char]::IsWhiteSpace($Text[$index])) {
                break
            }
            $index++
        }
        $destination = $Text.Substring($destinationStart, $index - $destinationStart)
    }

    while ($index -lt $Text.Length -and [char]::IsWhiteSpace($Text[$index])) {
        $index++
    }

    if ($index -lt $Text.Length -and ($Text[$index] -eq [char]34 -or $Text[$index] -eq [char]39 -or $Text[$index] -eq [char]40)) {
        $title = Read-LinkTitle -Text $Text -Start $index
        if (-not $title.Success) {
            return @{ Success = $false }
        }
        $index = $title.End
        while ($index -lt $Text.Length -and [char]::IsWhiteSpace($Text[$index])) {
            $index++
        }
    }

    if ($index -lt $Text.Length -and $Text[$index] -eq [char]41) {
        return @{ Success = $true; Target = $destination }
    }

    return @{ Success = $false }
}

function Get-InlineLinkTargets {
    param([string]$Text)

    $targets = New-Object 'System.Collections.Generic.List[string]'
    $index = 0

    while ($index -lt $Text.Length) {
        if ($Text[$index] -ne [char]91) {
            $index++
            continue
        }

        if (Test-IsEscaped -Text $Text -Index $index) {
            $index++
            continue
        }

        $depth = 0
        $cursor = $index
        while ($cursor -lt $Text.Length) {
            if ($Text[$cursor] -eq [char]92) {
                $cursor += 2
                continue
            }
            if ($Text[$cursor] -eq [char]91) {
                $depth++
            }
            elseif ($Text[$cursor] -eq [char]93) {
                $depth--
                if ($depth -eq 0) {
                    break
                }
            }
            $cursor++
        }

        if ($depth -ne 0) {
            $index++
            continue
        }

        $labelEnd = $cursor
        if ($labelEnd + 1 -ge $Text.Length -or $Text[$labelEnd + 1] -ne [char]40) {
            $index++
            continue
        }

        $destination = Read-LinkDestination -Text $Text -Start ($labelEnd + 2)
        if ($destination.Success -and $destination.Target.Length -gt 0) {
            $targets.Add($destination.Target)
        }

        $index++
    }

    return $targets
}

function Get-MarkdownLinkTargets {
    param([string]$Text)

    $chars = $Text.ToCharArray()
    Hide-FencedCode -Chars $chars -Text $Text
    Hide-InlineCode -Chars $chars -Text $Text
    $visible = -join $chars
    Hide-HtmlComments -Chars $chars -Text $visible
    $visible = -join $chars

    $targets = New-Object 'System.Collections.Generic.List[string]'
    foreach ($target in (Get-InlineLinkTargets -Text $visible)) {
        $targets.Add($target)
    }
    foreach ($match in $referenceDefinitionPattern.Matches($visible)) {
        $targets.Add($match.Groups['target'].Value)
    }

    return $targets
}

function Invoke-LinkParserSelfTest {
    $cases = @(
        @{
            Name = 'plain inline link'
            Text = '[plain](docs/a.md)'
            Expect = @('docs/a.md')
            Reject = @()
        },
        @{
            Name = 'nested label brackets'
            Text = '[nested [label]](docs/a.md)'
            Expect = @('docs/a.md')
            Reject = @()
        },
        @{
            Name = 'badge outer link and image'
            Text = '[![status](https://example.com/badge.svg)](missing.md)'
            Expect = @('missing.md', 'https://example.com/badge.svg')
            Reject = @()
        },
        @{
            Name = 'balanced destination parentheses'
            Text = '[paren](docs/a_(b).md)'
            Expect = @('docs/a_(b).md')
            Reject = @()
        },
        @{
            Name = 'balanced destination parentheses with title'
            Text = '[paren title](docs/a_(b).md "title")'
            Expect = @('docs/a_(b).md')
            Reject = @()
        },
        @{
            Name = 'angle destination with spaces'
            Text = '[angle](<docs/a b.md>)'
            Expect = @('docs/a b.md')
            Reject = @()
        },
        @{
            Name = 'outer and inner links'
            Text = '[outer [inner](docs/inner.md)](docs/outer.md)'
            Expect = @('docs/inner.md', 'docs/outer.md')
            Reject = @()
        },
        @{
            Name = 'four backtick fence hides inner three backticks'
            Text = @'
````text
```
[bad](missing.md)
```
````
'@
            Expect = @()
            Reject = @('missing.md')
        },
        @{
            Name = 'four backtick fence resumes outside'
            Text = @'
````text
[inside](hidden.md)
````
[outside](visible.md)
'@
            Expect = @('visible.md')
            Reject = @('hidden.md')
        },
        @{
            Name = 'single backtick code span'
            Text = 'Inline `code [hidden](hidden.md)` then [outside](visible.md)'
            Expect = @('visible.md')
            Reject = @('hidden.md')
        },
        @{
            Name = 'two backtick span contains one backtick'
            Text = 'Inline ``code ` [hidden](hidden.md) `` then [outside](visible.md)'
            Expect = @('visible.md')
            Reject = @('hidden.md')
        },
        @{
            Name = 'four backtick span contains three backticks'
            Text = 'Inline ````code ``` [hidden](hidden.md) ```` then [outside](visible.md)'
            Expect = @('visible.md')
            Reject = @('hidden.md')
        },
        @{
            Name = 'multiline inline code span'
            Text = @'
Inline `code
[hidden](hidden.md)
code` then [outside](visible.md)
'@
            Expect = @('visible.md')
            Reject = @('hidden.md')
        },
        @{
            Name = 'escaped opening bracket'
            Text = '\[bad](missing.md)'
            Expect = @()
            Reject = @('missing.md')
        },
        @{
            Name = 'missing closing parenthesis'
            Text = '[bad](missing.md'
            Expect = @()
            Reject = @('missing.md')
        },
        @{
            Name = 'space before destination'
            Text = '[bad] (missing.md)'
            Expect = @()
            Reject = @('missing.md')
        }
    )

    foreach ($case in $cases) {
        $actual = @(Get-MarkdownLinkTargets -Text $case.Text)
        foreach ($expected in @($case.Expect)) {
            if ($actual -notcontains $expected) {
                Stop-Check "link parser self-test '$($case.Name)' did not find '$expected'"
            }
        }
        foreach ($rejected in @($case.Reject)) {
            if ($actual -contains $rejected) {
                Stop-Check "link parser self-test '$($case.Name)' incorrectly found '$rejected'"
            }
        }
    }

    Write-Host "[check] link parser self-test OK ($($cases.Count) cases)"
}

function Test-LocalLink {
    param(
        [string]$Target,
        [string]$SourcePath
    )

    $value = $Target.Trim()
    if ($value.StartsWith('<') -and $value.EndsWith('>')) {
        $value = $value.Substring(1, $value.Length - 2).Trim()
    }

    if ($value.Length -eq 0) {
        return $true
    }

    if ($value.StartsWith('#')) {
        return $true
    }

    if ($value.StartsWith('//')) {
        return $true
    }

    if ($value -match '^[A-Za-z][A-Za-z0-9+.-]*:') {
        if ($value -notmatch '^[A-Za-z]:[\\/]') {
            return $true
        }
    }

    $fragmentIndex = $value.IndexOf('#')
    if ($fragmentIndex -ge 0) {
        $value = $value.Substring(0, $fragmentIndex)
    }

    $queryIndex = $value.IndexOf('?')
    if ($queryIndex -ge 0) {
        $value = $value.Substring(0, $queryIndex)
    }

    if ($value.Length -eq 0) {
        return $true
    }

    try {
        $value = [System.Uri]::UnescapeDataString($value)
    }
    catch {
        return $false
    }

    if ($value.StartsWith('/')) {
        $resolved = Join-Path $repoRoot $value.TrimStart('/')
    }
    elseif ($value -match '^[A-Za-z]:[\\/]') {
        $resolved = $value
    }
    else {
        $sourceDirectory = Split-Path -Parent $SourcePath
        $resolved = Join-Path $sourceDirectory $value
    }

    try {
        $resolved = [System.IO.Path]::GetFullPath($resolved)
    }
    catch {
        return $false
    }

    return Test-Path -LiteralPath $resolved
}

Write-Host "[check] repository: $repoRoot"

Invoke-NativeCommand 'cargo' @('fmt', '--check')
Invoke-NativeCommand 'cargo' @('check', '--all-targets', '--locked')
Invoke-NativeCommand 'cargo' @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings')
Invoke-NativeCommand 'cargo' @('test', '--locked')
Invoke-NativeCommand 'cargo' @('build', '--release', '--locked')

Invoke-LinkParserSelfTest

$markdownFiles = @(Get-ChildItem -LiteralPath $repoRoot -Recurse -File -Filter '*.md' |
    Where-Object { $_.FullName -notmatch '[\\/](target|\.git|node_modules)[\\/]' })

if ($markdownFiles.Count -eq 0) {
    Stop-Check 'no Markdown files were found'
}

$missingLinks = New-Object 'System.Collections.Generic.List[string]'
$checkedLinks = 0

foreach ($file in $markdownFiles) {
    $text = [System.IO.File]::ReadAllText($file.FullName)
    $targets = @(Get-MarkdownLinkTargets -Text $text)
    $seen = New-Object 'System.Collections.Generic.HashSet[string]'

    foreach ($target in $targets) {
        if (-not $seen.Add($target)) {
            continue
        }
        $checkedLinks++
        if (-not (Test-LocalLink -Target $target -SourcePath $file.FullName)) {
            $missingLinks.Add("$($file.FullName): $target")
        }
    }
}

if ($missingLinks.Count -gt 0) {
    Write-Host "[check] FAILED: $($missingLinks.Count) Markdown link target(s) are missing:" -ForegroundColor Red
    foreach ($missing in $missingLinks) {
        Write-Host "  $missing" -ForegroundColor Red
    }
    exit 1
}

Write-Host "[check] Markdown links OK ($checkedLinks link(s) in $($markdownFiles.Count) file(s))"
Write-Host "[check] all gates passed"
