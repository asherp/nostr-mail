use mock_email::{EmailStore, SmtpServer, ImapServer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

/// Helper to find an available port
async fn find_available_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Helper to read SMTP response (handles multi-line responses)
async fn read_smtp_response(reader: &mut BufReader<tokio::net::tcp::ReadHalf<'_>>) -> String {
    let mut last_line = String::new();
    loop {
        let mut line = String::new();
        // Add timeout to prevent hanging
        match timeout(Duration::from_secs(2), reader.read_line(&mut line)).await {
            Ok(Ok(bytes_read)) => {
                if bytes_read == 0 {
                    break;
                }
            }
            Ok(Err(e)) => panic!("Error reading SMTP response: {}", e),
            Err(_) => panic!("Timeout reading SMTP response"),
        }
        let trimmed = line.trim().to_string();
        last_line = trimmed.clone();
        // Multi-line SMTP responses use "250-" for continuation and "250 " or "250\r\n" for end
        // Check if this line ends the multi-line response (doesn't have "-" after the code)
        if trimmed.len() >= 4 {
            if trimmed.len() == 3 || (trimmed.len() > 3 && trimmed.chars().nth(3) != Some('-')) {
                break;
            }
        } else {
            break;
        }
    }
    last_line
}

/// Helper to read IMAP response (may be multiple lines)
async fn read_imap_response(reader: &mut BufReader<tokio::net::tcp::ReadHalf<'_>>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut max_lines = 20; // Prevent infinite loops
    let mut seen_tagged_response = false;
    
    loop {
        if max_lines == 0 {
            panic!("Too many lines in IMAP response, possible infinite loop. Lines so far: {:?}", lines);
        }
        max_lines -= 1;
        
        let mut line = String::new();
        // Add timeout to prevent hanging
        let bytes_read = match timeout(Duration::from_secs(2), reader.read_line(&mut line)).await {
            Ok(Ok(bytes_read)) => bytes_read,
            Ok(Err(e)) => panic!("Error reading IMAP response: {}", e),
            Err(_) => panic!("Timeout reading IMAP response. Lines so far: {:?}", lines),
        };
        
        if bytes_read == 0 {
            break; // Connection closed
        }
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue; // Skip empty lines but don't break (might be part of response)
        }
        lines.push(trimmed.clone());
        
        // IMAP responses can be:
        // - Untagged: start with "*" (e.g., "* OK ...", "* CAPABILITY ...")
        // - Tagged: start with tag like "a001" (e.g., "a001 OK ...")
        // - Continuation: start with "+" (for authentication)
        // Responses end with a tagged response (tag + OK/NO/BAD)
        
        if trimmed.starts_with("*") {
            // Untagged response - if it's "* OK" or "* NO" or "* BAD", it might be complete
            // But usually we wait for a tagged response. However, for greetings, 
            // "* OK" is the complete response, so break if we've seen at least one line
            // and this looks like a complete untagged response
            if lines.len() == 1 && (trimmed.contains(" OK") || trimmed.contains(" NO") || trimmed.contains(" BAD")) {
                // Single untagged response (like greeting) - this is complete
                break;
            }
            // Otherwise continue reading for more untagged lines
            continue;
        } else if trimmed.starts_with("+") {
            // Continuation response - continue reading
            continue;
        } else {
            // This is likely a tagged response (starts with tag like "a001")
            // Check if it contains OK/NO/BAD
            if trimmed.contains("OK") || trimmed.contains("NO") || trimmed.contains("BAD") {
                seen_tagged_response = true;
                break;
            }
        }
    }
    
    if lines.is_empty() {
        panic!("No lines read from IMAP response");
    }
    
    lines
}

#[tokio::test]
async fn test_smtp_basic_connection() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let smtp_port = find_available_port().await;
    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    
    // Start server in background
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect to SMTP server
    let mut stream = TcpStream::connect(smtp_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    // Read greeting
    let greeting = read_smtp_response(&mut reader).await;
    assert!(greeting.starts_with("220"));
    
    // Send EHLO
    writer.write_all(b"EHLO localhost\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    // Send QUIT
    writer.write_all(b"QUIT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("221"), "Expected response starting with '221', got: '{}'", response);
    
    smtp_handle.abort();
}

#[tokio::test]
async fn test_smtp_send_email() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let smtp_port = find_available_port().await;
    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    
    // Start server in background
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect and send email
    let mut stream = TcpStream::connect(smtp_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    // Read greeting
    read_smtp_response(&mut reader).await;
    
    // EHLO
    writer.write_all(b"EHLO localhost\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    // MAIL FROM
    writer.write_all(b"MAIL FROM:<sender@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    // RCPT TO
    writer.write_all(b"RCPT TO:<recipient@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    // DATA
    writer.write_all(b"DATA\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("354"));
    
    // Email content
    writer.write_all(b"From: sender@example.com\r\n").await.unwrap();
    writer.write_all(b"To: recipient@example.com\r\n").await.unwrap();
    writer.write_all(b"Subject: Test Email\r\n").await.unwrap();
    writer.write_all(b"\r\n").await.unwrap();
    writer.write_all(b"This is a test email body.\r\n").await.unwrap();
    writer.write_all(b".\r\n").await.unwrap();
    writer.flush().await.unwrap();
    
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"), "Expected 250 OK, got: {}", response);
    
    // QUIT
    writer.write_all(b"QUIT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    // Verify email was stored
    sleep(Duration::from_millis(100)).await;
    let emails = store.get_mailbox_emails("INBOX").await;
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].from.to_string(), "sender@example.com");
    assert_eq!(emails[0].to[0].to_string(), "recipient@example.com");
    assert_eq!(emails[0].subject, "Test Email");
    
    smtp_handle.abort();
}

#[tokio::test]
async fn test_smtp_multiple_recipients() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let smtp_port = find_available_port().await;
    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(smtp_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_smtp_response(&mut reader).await;
    writer.write_all(b"EHLO localhost\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    writer.write_all(b"MAIL FROM:<sender@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    // Multiple recipients
    writer.write_all(b"RCPT TO:<recipient1@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    writer.write_all(b"RCPT TO:<recipient2@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    writer.write_all(b"DATA\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    writer.write_all(b"From: sender@example.com\r\n").await.unwrap();
    writer.write_all(b"To: recipient1@example.com, recipient2@example.com\r\n").await.unwrap();
    writer.write_all(b"Subject: Multi-recipient Test\r\n").await.unwrap();
    writer.write_all(b"\r\n").await.unwrap();
    writer.write_all(b"Test body\r\n").await.unwrap();
    writer.write_all(b".\r\n").await.unwrap();
    writer.flush().await.unwrap();
    
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    writer.write_all(b"QUIT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    // Verify emails were stored for both recipients
    sleep(Duration::from_millis(100)).await;
    let emails = store.get_mailbox_emails("INBOX").await;
    assert_eq!(emails.len(), 2);
    
    smtp_handle.abort();
}

#[tokio::test]
async fn test_smtp_reset() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let smtp_port = find_available_port().await;
    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(smtp_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_smtp_response(&mut reader).await;
    writer.write_all(b"EHLO localhost\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    writer.write_all(b"MAIL FROM:<sender@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    // RSET should reset the transaction
    writer.write_all(b"RSET\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    // After RSET, we should be able to start a new transaction
    writer.write_all(b"MAIL FROM:<newsender@example.com>\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_smtp_response(&mut reader).await;
    assert!(response.starts_with("250"));
    
    writer.write_all(b"QUIT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_smtp_response(&mut reader).await;
    
    smtp_handle.abort();
}

#[tokio::test]
async fn test_imap_basic_connection() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let imap_port = find_available_port().await;
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(imap_addr).await.unwrap();
    // Give the server a moment to send the greeting
    sleep(Duration::from_millis(50)).await;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    // Read greeting (server sends this immediately on connection)
    let greeting = read_imap_response(&mut reader).await;
    assert!(greeting.iter().any(|l| l.contains("OK")));
    
    // CAPABILITY
    writer.write_all(b"a001 CAPABILITY\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("CAPABILITY")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    // LOGOUT
    writer.write_all(b"a002 LOGOUT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("BYE")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    imap_handle.abort();
}

#[tokio::test]
async fn test_imap_login_and_list() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let imap_port = find_available_port().await;
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(imap_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_imap_response(&mut reader).await;
    
    // LOGIN
    writer.write_all(b"a001 LOGIN test@example.com password\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("OK")));
    
    // LIST
    writer.write_all(b"a002 LIST \"\" \"*\"\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    // Should list default mailboxes
    assert!(response.iter().any(|l| l.contains("INBOX") || l.contains("LIST")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    writer.write_all(b"a003 LOGOUT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    imap_handle.abort();
}

#[tokio::test]
async fn test_imap_select_and_search() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    // Add a test email first
    use mock_email::{Email, EmailAddress};
    let email = Email {
        id: "test-1".to_string(),
        from: EmailAddress::from_string("sender@example.com").unwrap(),
        to: vec![EmailAddress::from_string("recipient@example.com").unwrap()],
        cc: vec![],
        bcc: vec![],
        subject: "Test Subject".to_string(),
        body: "Test body".to_string(),
        html_body: None,
        headers: std::collections::HashMap::new(),
        created_at: chrono::Utc::now().timestamp(),
        attachments: vec![],
    };
    store.add_email(email, "INBOX").await;
    
    let imap_port = find_available_port().await;
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(imap_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_imap_response(&mut reader).await;
    
    writer.write_all(b"a001 LOGIN test@example.com password\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    // SELECT INBOX
    writer.write_all(b"a002 SELECT INBOX\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("OK")));
    
    // SEARCH ALL
    writer.write_all(b"a003 SEARCH ALL\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("SEARCH")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    writer.write_all(b"a004 LOGOUT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    imap_handle.abort();
}

#[tokio::test]
async fn test_imap_fetch_email() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    // Add a test email
    use mock_email::{Email, EmailAddress};
    let email = Email {
        id: "test-1".to_string(),
        from: EmailAddress::from_string("sender@example.com").unwrap(),
        to: vec![EmailAddress::from_string("recipient@example.com").unwrap()],
        cc: vec![],
        bcc: vec![],
        subject: "Test Subject".to_string(),
        body: "Test body content".to_string(),
        html_body: None,
        headers: std::collections::HashMap::new(),
        created_at: chrono::Utc::now().timestamp(),
        attachments: vec![],
    };
    store.add_email(email, "INBOX").await;
    
    let imap_port = find_available_port().await;
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(imap_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_imap_response(&mut reader).await;
    
    writer.write_all(b"a001 LOGIN test@example.com password\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    writer.write_all(b"a002 SELECT INBOX\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    // FETCH ENVELOPE
    writer.write_all(b"a003 FETCH 1 ENVELOPE\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("FETCH")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    // FETCH BODY[TEXT]
    writer.write_all(b"a004 FETCH 1 BODY[TEXT]\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("FETCH")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    writer.write_all(b"a005 LOGOUT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    imap_handle.abort();
}

#[tokio::test]
async fn test_imap_status() {
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    // Add test emails
    use mock_email::{Email, EmailAddress};
    for i in 0..3 {
        let email = Email {
            id: format!("test-{}", i),
            from: EmailAddress::from_string("sender@example.com").unwrap(),
            to: vec![EmailAddress::from_string("recipient@example.com").unwrap()],
            cc: vec![],
            bcc: vec![],
            subject: format!("Test {}", i),
            body: "Test body".to_string(),
            html_body: None,
            headers: std::collections::HashMap::new(),
            created_at: chrono::Utc::now().timestamp(),
            attachments: vec![],
        };
        store.add_email(email, "INBOX").await;
    }
    
    let imap_port = find_available_port().await;
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let mut stream = TcpStream::connect(imap_addr).await.unwrap();
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    
    read_imap_response(&mut reader).await;
    
    writer.write_all(b"a001 LOGIN test@example.com password\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    // STATUS
    writer.write_all(b"a002 STATUS INBOX (MESSAGES)\r\n").await.unwrap();
    writer.flush().await.unwrap();
    let response = read_imap_response(&mut reader).await;
    assert!(response.iter().any(|l| l.contains("STATUS")));
    assert!(response.iter().any(|l| l.contains("MESSAGES")));
    assert!(response.iter().any(|l| l.contains("OK")));
    
    writer.write_all(b"a003 LOGOUT\r\n").await.unwrap();
    writer.flush().await.unwrap();
    read_imap_response(&mut reader).await;
    
    imap_handle.abort();
}

#[tokio::test]
async fn test_smtp_to_imap_flow() {
    // Test the full flow: send email via SMTP, then fetch via IMAP
    let store = Arc::new(EmailStore::new());
    store.init().await;
    
    let smtp_port = find_available_port().await;
    let imap_port = find_available_port().await;
    
    let smtp_addr = SocketAddr::from(([127, 0, 0, 1], smtp_port));
    let imap_addr = SocketAddr::from(([127, 0, 0, 1], imap_port));
    
    let smtp_server = SmtpServer::new(smtp_addr, store.clone());
    let imap_server = ImapServer::new(imap_addr, store.clone());
    
    let smtp_handle = tokio::spawn(async move {
        smtp_server.start().await.unwrap();
    });
    
    let imap_handle = tokio::spawn(async move {
        imap_server.start().await.unwrap();
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Send email via SMTP
    let mut smtp_stream = TcpStream::connect(smtp_addr).await.unwrap();
    let (smtp_reader, mut smtp_writer) = smtp_stream.split();
    let mut smtp_reader = BufReader::new(smtp_reader);
    
    read_smtp_response(&mut smtp_reader).await;
    smtp_writer.write_all(b"EHLO localhost\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    read_smtp_response(&mut smtp_reader).await;
    
    smtp_writer.write_all(b"MAIL FROM:<sender@example.com>\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    read_smtp_response(&mut smtp_reader).await;
    
    smtp_writer.write_all(b"RCPT TO:<recipient@example.com>\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    read_smtp_response(&mut smtp_reader).await;
    
    smtp_writer.write_all(b"DATA\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    read_smtp_response(&mut smtp_reader).await;
    
    smtp_writer.write_all(b"From: sender@example.com\r\n").await.unwrap();
    smtp_writer.write_all(b"To: recipient@example.com\r\n").await.unwrap();
    smtp_writer.write_all(b"Subject: Integration Test\r\n").await.unwrap();
    smtp_writer.write_all(b"\r\n").await.unwrap();
    smtp_writer.write_all(b"This is an integration test email.\r\n").await.unwrap();
    smtp_writer.write_all(b".\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    
    let response = read_smtp_response(&mut smtp_reader).await;
    assert!(response.starts_with("250"));
    
    smtp_writer.write_all(b"QUIT\r\n").await.unwrap();
    smtp_writer.flush().await.unwrap();
    read_smtp_response(&mut smtp_reader).await;
    
    // Wait for email to be stored
    sleep(Duration::from_millis(200)).await;
    
    // Fetch email via IMAP
    let mut imap_stream = TcpStream::connect(imap_addr).await.unwrap();
    let (imap_reader, mut imap_writer) = imap_stream.split();
    let mut imap_reader = BufReader::new(imap_reader);
    
    read_imap_response(&mut imap_reader).await;
    
    imap_writer.write_all(b"a001 LOGIN recipient@example.com password\r\n").await.unwrap();
    imap_writer.flush().await.unwrap();
    read_imap_response(&mut imap_reader).await;
    
    imap_writer.write_all(b"a002 SELECT INBOX\r\n").await.unwrap();
    imap_writer.flush().await.unwrap();
    read_imap_response(&mut imap_reader).await;
    
    imap_writer.write_all(b"a003 SEARCH ALL\r\n").await.unwrap();
    imap_writer.flush().await.unwrap();
    let search_response = read_imap_response(&mut imap_reader).await;
    assert!(search_response.iter().any(|l| l.contains("SEARCH")));
    
    imap_writer.write_all(b"a004 FETCH 1 ENVELOPE\r\n").await.unwrap();
    imap_writer.flush().await.unwrap();
    let fetch_response = read_imap_response(&mut imap_reader).await;
    assert!(fetch_response.iter().any(|l| l.contains("FETCH")));
    // ENVELOPE format has email parts separately, so check for both parts
    let response_text = fetch_response.join(" ");
    assert!(response_text.contains("sender") && response_text.contains("example.com"), 
        "Expected sender email in ENVELOPE, got: {:?}", fetch_response);
    
    imap_writer.write_all(b"a005 LOGOUT\r\n").await.unwrap();
    imap_writer.flush().await.unwrap();
    read_imap_response(&mut imap_reader).await;
    
    smtp_handle.abort();
    imap_handle.abort();
}
