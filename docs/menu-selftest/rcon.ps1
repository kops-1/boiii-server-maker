param([int]$Port, [string]$Password, [string[]]$Commands, [int]$TimeoutMs = 1500)
# Sends console commands to a local server through its remote-console port and prints the answers.
$udp = New-Object Net.Sockets.UdpClient
$udp.Client.ReceiveTimeout = $TimeoutMs
$udp.Connect("127.0.0.1", $Port)
foreach ($cmd in $Commands) {
    $bytes = [byte[]](255,255,255,255) + [Text.Encoding]::ASCII.GetBytes("rcon $Password $cmd")
    [void]$udp.Send($bytes, $bytes.Length)
    $answer = ""
    try {
        while ($true) {
            $ep = New-Object Net.IPEndPoint ([Net.IPAddress]::Any, 0)
            $data = $udp.Receive([ref]$ep)
            $answer += [Text.Encoding]::UTF8.GetString($data, 4, $data.Length - 4)
            $udp.Client.ReceiveTimeout = 300
        }
    } catch { }
    $udp.Client.ReceiveTimeout = $TimeoutMs
    Start-Sleep -Milliseconds 1200
    "> $cmd"
    if ($answer) { ($answer -replace "^print\n", "").TrimEnd() } else { "(no answer)" }
}
$udp.Close()

