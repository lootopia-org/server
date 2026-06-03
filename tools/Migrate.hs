-- | Standalone migration tool.
--
-- Applies the schema derived from "Auth.Models" to the configured database.
-- Run it once before starting the server, and again whenever the schema
-- changes:
--
-- > cabal run auth-migrate
--
-- Keeping migrations out of the server's start-up path means booting a new
-- instance never races to alter tables, and migrations can be gated in CI/CD.
module Main (main) where

import Auth.Config   (cfgDatabaseUrl, loadConfig, loadDotEnv)
import Auth.Database (mkPool, runMigrations)
import qualified Data.ByteString.Char8 as BC

main :: IO ()
main = do
  loadDotEnv ".env"
  cfg  <- loadConfig
  putStrLn $ "Applying migrations to: " <> BC.unpack (cfgDatabaseUrl cfg)
  pool <- mkPool cfg
  runMigrations pool
  putStrLn "Migrations applied."
