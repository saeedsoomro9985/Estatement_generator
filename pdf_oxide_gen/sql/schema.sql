-- Run against database [Statements]
USE [Statements];
GO

IF OBJECT_ID(N'dbo.Stmt_Request_Queue', N'U') IS NULL
BEGIN
    CREATE TABLE dbo.Stmt_Request_Queue (
        Id BIGINT IDENTITY(1,1) PRIMARY KEY,
        CIF VARCHAR(50) NOT NULL,
        Frequency VARCHAR(50) NOT NULL,
        Status INT NOT NULL DEFAULT 101,
        GeneratedStatus INT NOT NULL DEFAULT 109,
        MachineId VARCHAR(100) NULL,
        ProcessingStartedAt DATETIMEOFFSET NULL,
        GeneratedAt DATETIMEOFFSET NULL,
        CreatedAt DATETIMEOFFSET NOT NULL DEFAULT SYSDATETIMEOFFSET()
    );
    CREATE INDEX IX_Stmt_Request_Queue_GeneratedStatus_Id
        ON dbo.Stmt_Request_Queue (GeneratedStatus, Id);
END
GO
