module Auth.Config
  ( Config (..)
  , SmtpConfig (..)
  , loadConfig
  , loadDotEnv
  ) where

import           Control.Monad      (forM_, unless)
import           Data.Maybe         (fromMaybe)
import qualified Data.ByteString.Char8 as BC
import           Data.Text          (Text)
import qualified Data.Text          as T
import           System.Directory   (doesFileExist)
import           System.Environment (lookupEnv, setEnv)
import           Text.Read          (readMaybe)


data SmtpConfig = SmtpConfig
  { smtpHost :: String
  , smtpPort :: Int
  , smtpUser :: String
  , smtpPass :: String
  , smtpFrom :: Text
  } deriving (Show)

data Config = Config
  { cfgPort                 :: Int
  , cfgDatabaseUrl          :: BC.ByteString
  , cfgPasswordPepper       :: BC.ByteString
  , cfgPbkdf2Iterations     :: Int
  , cfgRpName               :: Text
  , cfgRpId                 :: Text
  , cfgOrigin               :: Text
  , cfgPublicBaseUrl        :: Text
  , cfgSessionTtlSeconds    :: Int
  , cfgEmailVerifyTtlSeconds :: Int
  , cfgCeremonyTtlSeconds   :: Int
  , cfgRequireVerifiedEmail :: Bool
  , cfgSmtp                 :: Maybe SmtpConfig
  } deriving (Show)

loadDotEnv :: FilePath -> IO ()
loadDotEnv path = do
  exists <- doesFileExist path
  unless (not exists) $ do
    contents <- readFile path
    forM_ (lines contents) $ \rawLine -> do
      let ln = trim rawLine
      case parseLine ln of
        Nothing         -> pure ()
        Just (key, val) -> do
          existing <- lookupEnv key
          maybe (setEnv key val) (const (pure ())) existing
  where
    parseLine l
      | null l            = Nothing
      | head l == '#'     = Nothing
      | otherwise         = case break (== '=') l of
          (_, [])        -> Nothing
          (k, _:v)       -> Just (trim k, unquote (trim v))
    unquote s = case s of
      ('"':rest)  | not (null rest) && last rest == '"'  -> init rest
      ('\'':rest) | not (null rest) && last rest == '\'' -> init rest
      _ -> s

trim :: String -> String
trim = f . f where f = reverse . dropWhile (`elem` (" \t\r\n" :: String))

loadConfig :: IO Config
loadConfig = do
  port       <- readEnv "PORT" 8080
  dbUrl      <- envStr "DATABASE_URL" "host=localhost port=5432 dbname=authdb user=postgres password=postgres"
  pepper     <- envStr "PASSWORD_PEPPER" "dev-only-insecure-pepper-change-me"
  iters      <- readEnv "PBKDF2_ITERATIONS" 200000
  rpName     <- envTxt "RP_NAME" "Haskell Auth Server"
  rpId       <- envTxt "RP_ID" "localhost"
  origin     <- envTxt "ORIGIN" "http://localhost:8080"
  baseUrl    <- envTxt "PUBLIC_BASE_URL" "http://localhost:8080"
  sessTtl    <- readEnv "SESSION_TTL_SECONDS" (60 * 60 * 24)
  verifyTtl  <- readEnv "EMAIL_VERIFY_TTL_SECONDS" (60 * 60 * 24)
  ceremTtl   <- readEnv "CEREMONY_TTL_SECONDS" 300
  requireVer <- readEnv "REQUIRE_VERIFIED_EMAIL" True
  smtp       <- loadSmtp
  pure Config
    { cfgPort                  = port
    , cfgDatabaseUrl           = BC.pack dbUrl
    , cfgPasswordPepper        = BC.pack pepper
    , cfgPbkdf2Iterations      = iters
    , cfgRpName                = rpName
    , cfgRpId                  = rpId
    , cfgOrigin                = origin
    , cfgPublicBaseUrl         = baseUrl
    , cfgSessionTtlSeconds     = sessTtl
    , cfgEmailVerifyTtlSeconds = verifyTtl
    , cfgCeremonyTtlSeconds    = ceremTtl
    , cfgRequireVerifiedEmail  = requireVer
    , cfgSmtp                  = smtp
    }

loadSmtp :: IO (Maybe SmtpConfig)
loadSmtp = do
  mHost <- lookupEnv "SMTP_HOST"
  case mHost of
    Nothing   -> pure Nothing
    Just ""   -> pure Nothing
    Just host -> do
      sport <- readEnv "SMTP_PORT" 587
      user  <- envStr "SMTP_USER" ""
      pass  <- envStr "SMTP_PASS" ""
      from  <- envTxt "SMTP_FROM" "no-reply@localhost"
      pure $ Just SmtpConfig
        { smtpHost = host, smtpPort = sport, smtpUser = user, smtpPass = pass, smtpFrom = from }

readEnv :: Read a => String -> a -> IO a
readEnv key def = do
  mv <- lookupEnv key
  pure $ fromMaybe def (mv >>= readMaybe . normaliseBool)
  where
    normaliseBool s = case s of
      "true"  -> "True"
      "false" -> "False"
      other   -> other

envStr :: String -> String -> IO String
envStr key def = fromMaybe def <$> lookupEnv key

envTxt :: String -> Text -> IO Text
envTxt key def = maybe def T.pack <$> lookupEnv key
