%% psg-article.cls -- research article class for Panorama Scholarly Group journals.
%% Part of PSG Academic PDF. Licensed MIT OR Apache-2.0, like the rest of the repository.
%% Written for pdfLaTeX; the bibliography uses BibLaTeX with the Biber backend.
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{psg-article}[2026/11/12 v1.0.0 PSG research article]

\LoadClass[10pt,a4paper,twoside]{article}

%% Fonts and encoding (pdfLaTeX).
\RequirePackage[T1]{fontenc}
\RequirePackage{lmodern}
\RequirePackage{microtype}

%% Page layout.
\RequirePackage[a4paper,top=25mm,bottom=25mm,inner=22mm,outer=22mm,headsep=8mm,footskip=10mm]{geometry}

%% Common content packages, so converted manuscripts compile without extra setup.
\RequirePackage{amsmath,amssymb}
\RequirePackage{graphicx}
\RequirePackage{booktabs}
\RequirePackage{longtable}
\RequirePackage{array}
\RequirePackage{calc}
\RequirePackage[table]{xcolor}
\RequirePackage[font=small,labelfont=bf]{caption}
\RequirePackage{fancyhdr}

%% Bibliography: BibLaTeX with Biber.
\RequirePackage{csquotes}
\RequirePackage[backend=biber,style=numeric-comp,sorting=none,maxbibnames=10]{biblatex}

\RequirePackage[hidelinks,unicode]{hyperref}

\definecolor{psgaccent}{RGB}{31,58,104}

%% Pandoc's LaTeX writer uses \tightlist in lists.
\providecommand{\tightlist}{\setlength{\itemsep}{0pt}\setlength{\parskip}{0pt}}

%% ---------------------------------------------------------------------------
%% Publication metadata. PSG Academic PDF writes psg-metadata.tex, which calls
%% these setters; every value has already been escaped for LaTeX.
%% ---------------------------------------------------------------------------
\newcommand*\psg@journal{PSG Journal}
\newcommand*\psg@journalabbrev{}
\newcommand*\psg@issn{}
\newcommand*\psg@volume{}
\newcommand*\psg@issue{}
\newcommand*\psg@year{}
\newcommand*\psg@pages{}
\newcommand*\psg@articlenumber{}
\newcommand*\psg@doi{}
\newcommand*\psg@articletype{Research Article}
\newcommand*\psg@title{Untitled}
\newcommand*\psg@shorttitle{}
\newcommand\psg@authors{}
\newcommand\psg@affiliations{}
\newcommand\psg@correspondence{}
\newcommand\psg@abstract{}
\newcommand\psg@keywords{}
\newcommand*\psg@received{}
\newcommand*\psg@revised{}
\newcommand*\psg@accepted{}
\newcommand*\psg@published{}
\newcommand\psg@licence{}
\newcommand\psg@copyright{}
\newcommand*\psg@pdftitle{}
\newcommand*\psg@pdfauthor{}
\newcommand*\psg@pdfkeywords{}

\newcommand\psgJournal[1]{\renewcommand*\psg@journal{#1}}
\newcommand\psgJournalAbbrev[1]{\renewcommand*\psg@journalabbrev{#1}}
\newcommand\psgISSN[1]{\renewcommand*\psg@issn{#1}}
\newcommand\psgVolume[1]{\renewcommand*\psg@volume{#1}}
\newcommand\psgIssue[1]{\renewcommand*\psg@issue{#1}}
\newcommand\psgYear[1]{\renewcommand*\psg@year{#1}}
\newcommand\psgPages[1]{\renewcommand*\psg@pages{#1}}
\newcommand\psgArticleNumber[1]{\renewcommand*\psg@articlenumber{#1}}
\newcommand\psgDOI[1]{\renewcommand*\psg@doi{#1}}
\newcommand\psgArticleType[1]{\renewcommand*\psg@articletype{#1}}
\newcommand\psgTitle[1]{\renewcommand*\psg@title{#1}}
\newcommand\psgShortTitle[1]{\renewcommand*\psg@shorttitle{#1}}
\newcommand\psgAuthors[1]{\renewcommand\psg@authors{#1}}
\newcommand\psgAffiliations[1]{\renewcommand\psg@affiliations{#1}}
\newcommand\psgCorrespondence[1]{\renewcommand\psg@correspondence{#1}}
\newcommand\psgAbstract[1]{\renewcommand\psg@abstract{#1}}
\newcommand\psgKeywords[1]{\renewcommand\psg@keywords{#1}}
\newcommand\psgReceived[1]{\renewcommand*\psg@received{#1}}
\newcommand\psgRevised[1]{\renewcommand*\psg@revised{#1}}
\newcommand\psgAccepted[1]{\renewcommand*\psg@accepted{#1}}
\newcommand\psgPublished[1]{\renewcommand*\psg@published{#1}}
\newcommand\psgLicence[1]{\renewcommand\psg@licence{#1}}
\newcommand\psgCopyright[1]{\renewcommand\psg@copyright{#1}}
\newcommand\psgPdfTitle[1]{\renewcommand*\psg@pdftitle{#1}}
\newcommand\psgPdfAuthor[1]{\renewcommand*\psg@pdfauthor{#1}}
\newcommand\psgPdfKeywords[1]{\renewcommand*\psg@pdfkeywords{#1}}

%% Author superscripts, used by the generated author list.
\newcommand\psgAffMark[1]{\textsuperscript{#1}}

\AtBeginDocument{%
  \hypersetup{pdftitle={\psg@pdftitle},pdfauthor={\psg@pdfauthor},pdfkeywords={\psg@pdfkeywords},pdfsubject={\psg@journal}}%
}

%% Citation line used in the running header and the title block.
\newcommand\psg@citeline{%
  \textit{\psg@journal}%
  \ifx\psg@year\@empty\else\space\psg@year\fi
  \ifx\psg@volume\@empty\else, \textbf{\psg@volume}\fi
  \ifx\psg@issue\@empty\else(\psg@issue)\fi
  \ifx\psg@pages\@empty\else, \psg@pages\fi
  \ifx\psg@articlenumber\@empty\else, \psg@articlenumber\fi}

%% Running headers and footers.
\pagestyle{fancy}
\fancyhf{}
\renewcommand{\headrulewidth}{0.4pt}
\fancyhead[LE]{\small\psg@citeline}
\fancyhead[RO]{\small\ifx\psg@shorttitle\@empty\psg@title\else\psg@shorttitle\fi}
\fancyhead[LO,RE]{\small\thepage}
\fancyfoot[C]{\footnotesize\ifx\psg@doi\@empty\else\href{https://doi.org/\psg@doi}{https://doi.org/\psg@doi}\fi}
% No \par in a fancyhdr argument: fancyhdr before 4.0 (TeX Live 2021) reads it as a short macro.
\fancypagestyle{psgfirst}{%
  \fancyhf{}%
  \renewcommand{\headrulewidth}{0pt}%
  \fancyfoot[C]{\parbox[b]{\textwidth}{\footnotesize\raggedright
    \ifx\psg@copyright\@empty\else\psg@copyright\\\fi
    \psg@licence}}%
}

%% Section headings in the accent colour.
\renewcommand\section{\@startsection{section}{1}{\z@}%
  {-3ex \@plus -1ex \@minus -.2ex}{1.5ex \@plus .2ex}%
  {\normalfont\large\bfseries\color{psgaccent}}}
\renewcommand\subsection{\@startsection{subsection}{2}{\z@}%
  {-2.5ex \@plus -1ex \@minus -.2ex}{1ex \@plus .2ex}%
  {\normalfont\normalsize\bfseries\color{psgaccent}}}

%% The title block.
\newcommand\psgmaketitle{%
  \thispagestyle{psgfirst}%
  \begingroup
  \noindent{\color{psgaccent}\rule{\textwidth}{2pt}}\par\vspace{2pt}%
  \noindent{\small\textsc{\psg@journal}%
    \ifx\psg@issn\@empty\else\hfill ISSN \psg@issn\fi}\par
  \noindent{\small\psg@citeline}\par
  \ifx\psg@doi\@empty\else\noindent{\small DOI: \href{https://doi.org/\psg@doi}{\psg@doi}}\par\fi
  \vspace{10pt}%
  \noindent{\small\color{psgaccent}\MakeUppercase{\psg@articletype}}\par\vspace{4pt}%
  \noindent{\LARGE\bfseries\raggedright\psg@title\par}%
  \vspace{10pt}%
  \noindent{\normalsize\raggedright\psg@authors\par}%
  \vspace{6pt}%
  \noindent\begin{minipage}{\textwidth}\footnotesize\raggedright\psg@affiliations
    \ifx\psg@correspondence\@empty\else\par\smallskip\psg@correspondence\fi\end{minipage}\par
  \vspace{8pt}%
  \noindent\begin{minipage}{\textwidth}\footnotesize
    \ifx\psg@received\@empty\else Received: \psg@received\quad\fi
    \ifx\psg@revised\@empty\else Revised: \psg@revised\quad\fi
    \ifx\psg@accepted\@empty\else Accepted: \psg@accepted\quad\fi
    \ifx\psg@published\@empty\else Published: \psg@published\fi
  \end{minipage}\par
  \vspace{10pt}%
  \noindent\colorbox{psgaccent!6}{\begin{minipage}{\textwidth-2\fboxsep}
    \small\textbf{Abstract.}\space\psg@abstract
    \ifx\psg@keywords\@empty\else\par\smallskip\textbf{Keywords:}\space\psg@keywords\fi
  \end{minipage}}\par
  \vspace{14pt}%
  \endgroup}

\endinput
